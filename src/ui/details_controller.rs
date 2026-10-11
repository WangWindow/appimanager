//! Application detail state, rendering and per-app preference persistence.

use super::*;

pub fn reload_current_metadata(state: &UiState) {
    let Some(app) = state.current_detail.borrow().clone() else {
        return;
    };
    let settings = state.settings.borrow().clone();
    state.detail_reload.set_sensitive(false);
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = service::reload_metadata(&app, &settings).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
    let state_clone = state.clone();
    glib::timeout_add_local(
        std::time::Duration::from_millis(50),
        move || match receiver.try_recv() {
            Ok(result) => {
                state_clone.detail_reload.set_sensitive(true);
                match result {
                    Ok(updated) => {
                        if let Some(stored) = state_clone
                            .apps
                            .borrow_mut()
                            .iter_mut()
                            .find(|stored| stored.appimage_path == updated.appimage_path)
                        {
                            *stored = updated.clone();
                        }
                        show_details(&state_clone, updated);
                    }
                    Err(error) => show_message(&state_clone, &i18n::tr("Unknown Error"), &error),
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        },
    );
}

pub fn filter_rows(state: &UiState, _query: &str) {
    render_apps(state);
}

pub fn show_details(state: &UiState, app: InstalledApp) {
    state.detail_is_candidate.set(false);
    state.detail_advanced.set_visible(true);
    state.detail_reload_row.set_visible(true);
    state.detail_website_row.set_visible(true);
    state
        .detail_icon
        .set_tooltip_text(Some(app.appimage_type.label()));
    *state.current_detail.borrow_mut() = Some(app.clone());
    state.title.set_text(&app.name);
    state
        .subtitle
        .set_text(app.description.as_deref().unwrap_or(""));
    if let Some(path) = app.icon_path.as_ref().filter(|p| p.is_file()) {
        state.detail_icon.set_from_file(Some(path));
    } else if let Some(icon_name) = app.icon_name.as_deref() {
        state.detail_icon.set_icon_name(Some(icon_name));
    } else {
        state
            .detail_icon
            .set_icon_name(Some("application-x-executable-symbolic"));
    }
    state.detail_pills.remove_all();
    if let Some(version) = app.version.as_deref().filter(|version| !version.is_empty()) {
        append_pill(&state.detail_pills, version);
    }
    append_pill(&state.detail_pills, &format_size(app.size));
    append_pill(
        &state.detail_pills,
        &format!(
            "AppImage Type. {}",
            match app.appimage_type {
                model::AppImageType::Type1 => "1",
                model::AppImageType::Type2 => "2",
                model::AppImageType::Unknown => "0",
            }
        ),
    );
    state.detail_primary.set_label(&i18n::tr("Remove"));
    state
        .detail_primary
        .set_css_classes(&["big-btn", "text-button", "destructive-action"]);
    state.detail_primary.set_sensitive(true);
    state.detail_banner.set_button_label(None);
    let current = Architecture::current();
    if app.architecture != Architecture::Unknown && current != app.architecture {
        state.detail_banner.set_title(&i18n::tr(
            "This app might not be compatible with your system architecture",
        ));
        state.detail_banner.set_revealed(true);
    } else if app.external_folder {
        state.detail_banner.set_title(&i18n::tr(
            "This app is located outside the default folder. You can hide external apps in Preferences.",
        ));
        state.detail_banner.set_revealed(true);
    } else {
        state.detail_banner.set_revealed(false);
    }
    while let Some(child) = state.detail_info.first_child() {
        state.detail_info.remove(&child);
    }
    let path_row = adw::ActionRow::builder()
        .title(i18n::tr("Path"))
        .subtitle(app.appimage_path.to_string_lossy())
        .subtitle_selectable(true)
        .selectable(false)
        .build();
    path_row.add_prefix(&presentation::icon("folder", 34));
    let open_folder = presentation::icon_button("external-link", "Open Folder");
    open_folder.set_valign(gtk::Align::Center);
    let folder = app.appimage_path.parent().map(PathBuf::from);
    open_folder.connect_clicked(move |_| {
        if let Some(folder) = &folder {
            let launcher = gtk::FileLauncher::new(Some(&gio::File::for_path(folder)));
            launcher.launch(None::<&gtk::Window>, None::<&gio::Cancellable>, |_| {});
        }
    });
    path_row.add_suffix(&open_folder);
    let path_card = gtk::ListBox::new();
    path_card.add_css_class("boxed-list");
    path_card.set_selection_mode(gtk::SelectionMode::None);
    path_card.append(&path_row);
    state.detail_info.append(&path_card);
    state.detail_website_row.set_visible(true);
    if let Ok(app_config) = config::load_app_config(&app.appimage_path) {
        state.detail_website.set_title(&i18n::tr(
            if app_config.website.as_deref().unwrap_or_default().is_empty() {
                "Add a website"
            } else {
                "Website"
            },
        ));
        state
            .detail_website
            .set_text(app_config.website.as_deref().unwrap_or(""));
    } else {
        state.detail_website.set_text("");
    }
    let arguments = app
        .exec_arguments
        .iter()
        .map(|argument| desktop::quote_exec_arg(argument))
        .collect::<Vec<_>>()
        .join(" ");
    state.detail_arguments.set_text(&arguments);
    state.detail_environment.set_values(&app.environment);
    state.detail_launch.set_label(&i18n::tr(if app.terminal {
        "This app runs in the terminal"
    } else {
        "Launch"
    }));
    state.detail_launch.set_sensitive(!app.terminal);
    state.stack.set_visible_child_name("details");
}

fn append_pill(container: &adw::ToggleGroup, label: &str) {
    let pill = adw::Toggle::new();
    pill.set_label(Some(label));
    container.add(pill);
}

pub fn save_current_details(state: &UiState) {
    let Some(mut app) = state.current_detail.borrow().clone() else {
        return;
    };
    let argument_text = state.detail_arguments.text();
    let arguments = if argument_text.trim().is_empty() {
        Vec::new()
    } else {
        desktop::parse_exec(&format!("app {}", argument_text))
            .into_iter()
            .skip(1)
            .collect::<Vec<_>>()
    };

    let environment = match state.detail_environment.values() {
        Ok(values) => values,
        Err(error) => {
            show_message(state, &i18n::tr("Invalid data"), &error.to_string());
            return;
        }
    };

    let result = desktop::write_desktop_exec(
        &app.desktop_file_path,
        &app.appimage_path,
        &arguments,
        &environment,
    )
    .and_then(|_| {
        let mut app_config = config::load_app_config(&app.appimage_path)?;
        app_config.arguments = arguments.clone();
        app_config.environment = environment.clone();
        config::save_app_config(&app.appimage_path, &app_config)
    });
    if let Err(error) = result {
        show_message(state, &i18n::tr("Unknown Error"), &error.to_string());
        return;
    }

    app.exec_arguments = arguments;
    app.environment = environment;
    *state.current_detail.borrow_mut() = Some(app.clone());
    if let Some(stored) = state
        .apps
        .borrow_mut()
        .iter_mut()
        .find(|stored| stored.desktop_file_path == app.desktop_file_path)
    {
        *stored = app;
    }
}

pub fn save_website(state: &UiState) {
    let Some(app) = state.current_detail.borrow().clone() else {
        return;
    };
    let value = state.detail_website.text().trim().to_string();
    if !value.is_empty() && !(value.starts_with("http://") || value.starts_with("https://")) {
        show_message(
            state,
            &i18n::tr("Invalid data"),
            &i18n::tr("Enter a valid HTTP url"),
        );
        return;
    }
    persist_website(&app, value);
}

/// Autosaves only valid (or empty) website values when the entry loses focus.
/// Invalid text is left visible so activation can show the explanatory dialog.
pub fn save_website_if_valid(state: &UiState) {
    let Some(app) = state.current_detail.borrow().clone() else {
        return;
    };
    let value = state.detail_website.text().trim().to_string();
    if !value.is_empty() && !(value.starts_with("http://") || value.starts_with("https://")) {
        return;
    }
    persist_website(&app, value);
}

fn persist_website(app: &InstalledApp, value: String) {
    let mut app_config = config::load_app_config(&app.appimage_path).unwrap_or_default();
    app_config.website = (!value.is_empty()).then_some(value);
    let _ = config::save_app_config(&app.appimage_path, &app_config);
}

pub fn open_current_website(state: &UiState) {
    let value = state.detail_website.text().trim().to_string();
    if value.is_empty() {
        return;
    }
    let launcher = gtk::UriLauncher::new(&value);
    launcher.launch(
        Some(&state.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Err(error) = result {
                eprintln!("Unable to open website: {error}");
            }
        },
    );
}
