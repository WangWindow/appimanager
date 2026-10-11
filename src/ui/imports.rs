//! File opening, import preview, trust confirmation and integration flow.

use super::*;

/// Named widget handles for the multiple-import page.
pub struct ImportPage {
    pub page: gtk::ScrolledWindow,
    pub list: gtk::ListBox,
    pub install_all: gtk::Button,
    pub already_installed: gtk::Button,
}

pub fn build_import_page() -> ImportPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 20);
    content.add_css_class("page-content");
    content.set_margin_top(30);
    content.set_margin_bottom(10);
    content.set_margin_start(20);
    content.set_margin_end(20);
    let confirm = gtk::Button::with_label(&i18n::tr("Move all to the app menu"));
    confirm.add_css_class("suggested-action");
    confirm.set_halign(gtk::Align::Center);
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    let warning = gtk::Button::builder()
        .child(&warning_content())
        .sensitive(false)
        .visible(false)
        .css_classes(["flat"])
        .build();
    content.append(&confirm);
    content.append(&list);
    content.append(&warning);
    let clamp = adw::Clamp::builder()
        .child(&content)
        .maximum_size(600)
        .build();
    ImportPage {
        page: gtk::ScrolledWindow::builder()
            .child(&clamp)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build(),
        list,
        install_all: confirm,
        already_installed: warning,
    }
}

fn warning_content() -> gtk::Box {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    content.append(&presentation::icon("info", 16));
    content.append(&gtk::Label::new(Some(&i18n::tr(
        "Some apps are already installed",
    ))));
    content
}

pub fn choose_files(state: &UiState) {
    let dialog = gtk::FileDialog::builder()
        .title(i18n::tr("Open a file"))
        .modal(true)
        .build();
    let state_clone = state.clone();
    dialog.open_multiple(
        Some(&state.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Ok(files) = result {
                let paths = (0..files.n_items())
                    .filter_map(|index| files.item(index))
                    .filter_map(|object| object.downcast::<gio::File>().ok())
                    .filter_map(|file| file.path())
                    .collect();
                open_import_paths(&state_clone, paths);
            }
        },
    );
}

pub fn open_import_paths(state: &UiState, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    state.candidate_trusted.set(false);
    let safe = state.settings.borrow().use_safe_extraction;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut candidates = Vec::new();
        let mut errors = Vec::new();
        for path in paths {
            match appimage::inspect(&path, !safe) {
                Ok(candidate) => candidates.push(candidate),
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        let _ = sender.send((candidates, errors));
    });
    let state = state.clone();
    glib::timeout_add_local(
        std::time::Duration::from_millis(30),
        move || match receiver.try_recv() {
            Ok((candidates, errors)) => {
                if !candidates.is_empty() {
                    show_imports(&state, candidates);
                }
                if !errors.is_empty() {
                    show_message(&state, &i18n::tr("Unknown Error"), &errors.join("\n"));
                }
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        },
    );
}

fn show_imports(state: &UiState, candidates: Vec<ImportCandidate>) {
    *state.current_imports.borrow_mut() = candidates.clone();
    if candidates.len() == 1 {
        show_candidate(state, &candidates[0]);
        return;
    }
    while let Some(child) = state.import_list.first_child() {
        state.import_list.remove(&child);
    }
    let mut any_installed = false;
    for candidate in &candidates {
        let row = list::app_row(
            &candidate.name,
            candidate.description.as_deref(),
            candidate.version.as_deref(),
            candidate.size,
            candidate.icon_path.as_ref(),
            candidate.icon_name.as_deref(),
        );
        let installed = state
            .apps
            .borrow()
            .iter()
            .any(|app| app.appimage_path == candidate.path);
        if installed {
            row.set_opacity(0.5);
            any_installed = true;
        }
        let button = presentation::icon_button("arrow-right", "Open app details");
        button.set_valign(gtk::Align::Center);
        if let Some(line) = row
            .child()
            .and_then(|widget| widget.downcast::<gtk::Box>().ok())
        {
            line.append(&button);
        }
        let state_clone = state.clone();
        let candidate = candidate.clone();
        button.connect_clicked(move |_| show_candidate(&state_clone, &candidate));
        state.import_list.append(&row);
    }
    state.import_warning.set_visible(any_installed);
    state.detail_is_candidate.set(false);
    state.stack.set_visible_child_name("import");
}

pub fn show_candidate(state: &UiState, candidate: &ImportCandidate) {
    if let Some(app) = state
        .apps
        .borrow()
        .iter()
        .find(|app| app.appimage_path == candidate.path)
        .cloned()
    {
        show_details(state, app);
        return;
    }
    let app = InstalledApp {
        name: candidate.name.clone(),
        description: candidate.description.clone(),
        version: candidate.version.clone(),
        appimage_path: candidate.path.clone(),
        desktop_file_path: PathBuf::new(),
        icon_path: candidate.icon_path.clone(),
        icon_name: candidate.icon_name.clone(),
        exec_arguments: candidate.exec_arguments.clone(),
        environment: candidate.environment.clone(),
        architecture: candidate.architecture,
        appimage_type: candidate.appimage_type,
        size: candidate.size,
        terminal: candidate.terminal,
        external_folder: false,
        status: model::InstalledStatus::NotInstalled,
    };
    show_details(state, app);
    state.detail_is_candidate.set(true);
    state.detail_advanced.set_visible(false);
    state.detail_reload_row.set_visible(false);
    state.detail_website_row.set_visible(false);
    let hash_row = adw::ActionRow::builder()
        .title(i18n::tr("Hash"))
        .subtitle(format!(
            "md5: {}\nsha1: {}\nsha256: {}",
            candidate.hashes.md5, candidate.hashes.sha1, candidate.hashes.sha256
        ))
        .subtitle_lines(0)
        .subtitle_selectable(true)
        .selectable(true)
        .build();
    hash_row.add_prefix(&presentation::icon("hash", 34));
    let hash_card = gtk::ListBox::new();
    hash_card.add_css_class("boxed-list");
    hash_card.set_selection_mode(gtk::SelectionMode::None);
    hash_card.append(&hash_row);
    state.detail_info.append(&hash_card);
    state
        .detail_primary
        .set_label(&i18n::tr("Move to the app menu"));
    state
        .detail_primary
        .set_css_classes(&["big-btn", "text-button", "suggested-action"]);
    if !state.candidate_trusted.get() {
        state.detail_banner.set_title(&i18n::tr(
            "Please, verify the source of this app before opening it",
        ));
        state
            .detail_banner
            .set_button_label(Some(&i18n::tr("Unlock")));
        state.detail_banner.set_revealed(true);
    }
    state
        .detail_primary
        .set_sensitive(state.candidate_trusted.get());
    state
        .detail_launch
        .set_sensitive(state.candidate_trusted.get() && !candidate.terminal);
}

pub fn unlock_candidate(state: &UiState) {
    if !state.detail_is_candidate.get() {
        return;
    }
    state.candidate_trusted.set(true);
    state.detail_banner.set_revealed(false);
    state.detail_primary.set_sensitive(true);
    state.detail_launch.set_sensitive(
        state
            .current_detail
            .borrow()
            .as_ref()
            .is_some_and(|app| !app.terminal),
    );
}

// Import confirmation and integration controller.

pub fn install_current(state: &UiState) {
    if !state.candidate_trusted.get() {
        let dialog = adw::MessageDialog::builder()
            .transient_for(&state.window)
            .heading(i18n::tr("Verify app source"))
            .body(i18n::tr(
                "Do you really want to move all the apps to the menu?",
            ))
            .build();
        let trust =
            gtk::CheckButton::with_label(&i18n::tr("I have verified the source of the apps"));
        trust.set_margin_top(8);
        trust.set_margin_bottom(12);
        dialog.set_extra_child(Some(&trust));
        dialog.add_response("cancel", &i18n::tr("Cancel"));
        dialog.add_response("confirm", &i18n::tr("Proceed"));
        dialog.set_response_enabled("confirm", false);
        let dialog_weak = dialog.downgrade();
        trust.connect_toggled(move |check| {
            if let Some(dialog) = dialog_weak.upgrade() {
                dialog.set_response_enabled("confirm", check.is_active());
            }
        });
        let state_clone = state.clone();
        dialog.connect_response(None, move |dialog, response| {
            if response == "confirm" {
                state_clone.candidate_trusted.set(true);
                install_current(&state_clone);
            }
            dialog.close();
        });
        dialog.present();
        return;
    }
    let mut candidates = state.current_imports.borrow().clone();
    if state.detail_is_candidate.get()
        && let Some(current) = state.current_detail.borrow().as_ref()
    {
        candidates.retain(|candidate| candidate.path == current.appimage_path);
    }
    for candidate in &mut candidates {
        candidate.trusted = true;
    }
    if candidates.is_empty() {
        return;
    }
    let conflicts: Vec<InstalledApp> = state
        .apps
        .borrow()
        .iter()
        .filter(|app| {
            candidates
                .iter()
                .any(|candidate| candidate.name == app.name)
        })
        .cloned()
        .collect();
    if !conflicts.is_empty() {
        let dialog = adw::MessageDialog::builder()
            .transient_for(&state.window)
            .heading(i18n::tr("Some applications are already integrated"))
            .body(i18n::tr(
                "Keep both versions or replace the existing integration?",
            ))
            .build();
        dialog.add_response("cancel", &i18n::tr("Cancel"));
        dialog.add_response("keep", &i18n::tr("Keep both"));
        dialog.add_response("replace", &i18n::tr("Replace"));
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        let state_clone = state.clone();
        dialog.connect_response(None, move |dialog, response| {
            if response == "keep" || response == "replace" {
                start_install(&state_clone, candidates.clone(), response == "replace");
            }
            dialog.close();
        });
        dialog.present();
        return;
    }
    start_install(state, candidates, false);
}

fn start_install(state: &UiState, candidates: Vec<ImportCandidate>, replace: bool) {
    let settings = state.settings.borrow().clone();
    let existing = state.apps.borrow().clone();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let result = (|| -> anyhow::Result<()> {
            if replace {
                for existing_app in existing.iter().filter(|app| {
                    candidates
                        .iter()
                        .any(|candidate| candidate.name == app.name)
                }) {
                    service::remove_app(existing_app)?;
                }
            }
            candidates
                .iter()
                .try_for_each(|candidate| service::install_candidate(candidate, &settings))?;
            Ok(())
        })();
        let _ = sender.send(result);
    });
    let state_clone = state.clone();
    glib::timeout_add_local(
        std::time::Duration::from_millis(30),
        move || match receiver.try_recv() {
            Ok(result) => {
                match result {
                    Ok(_) => {
                        toast(
                            &state_clone,
                            &i18n::tr("AppImage added to the application menu"),
                        );
                        show_list(&state_clone);
                    }
                    Err(error) => toast(&state_clone, &error.to_string()),
                };
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        },
    );
}
