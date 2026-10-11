//! Installed-application list, filtering, discovery refresh and list rows.

use super::*;

/// Returns to the installed-application view and refreshes it from desktop entries.
pub fn show_list(state: &UiState) {
    state.stack.set_visible_child_name("list");
    refresh_apps(state);
}

/// Scans user desktop entries away from the GTK thread, then applies one update
/// to the list model when the result reaches the main context.
pub fn refresh_apps(state: &UiState) {
    let directory = state.settings.borrow().appimages_directory.clone();
    let show_external = state.settings.borrow().show_external;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(desktop::load_installed(&directory, show_external));
    });
    let state = state.clone();
    glib::timeout_add_local(
        std::time::Duration::from_millis(30),
        move || match receiver.try_recv() {
            Ok(Ok(apps)) => {
                *state.apps.borrow_mut() = apps;
                render_apps(&state);
                glib::ControlFlow::Break
            }
            Ok(Err(error)) => {
                dialogs::toast(&state, &error.to_string());
                glib::ControlFlow::Break
            }
            Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
            Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
        },
    );
}

pub fn render_apps(state: &UiState) {
    while let Some(child) = state.list.first_child() {
        state.list.remove(&child);
    }
    let apps = state.apps.borrow();
    if apps.is_empty() {
        state.list_content.set_visible_child_name("empty");
        return;
    }
    state.list_content.set_visible_child_name("applications");
    let query = state.search.text().to_lowercase().replace(' ', "");
    let mut count = 0;
    for app in apps
        .iter()
        .filter(|app| query.is_empty() || app.name.to_lowercase().contains(&query))
    {
        let row = app_row(
            &app.name,
            app.description.as_deref(),
            app.version.as_deref(),
            app.size,
            app.icon_path.as_ref(),
            app.icon_name.as_deref(),
        );
        state.list.append(&row);
        count += 1;
    }
    if count == 0 {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(false);
        row.set_selectable(false);
        let label = gtk::Label::new(Some(&i18n::tr("No apps found")));
        label.add_css_class("app-listbox-item");
        label.set_margin_top(20);
        label.set_margin_bottom(20);
        row.set_child(Some(&label));
        state.list.append(&row);
    }
}

pub fn app_row(
    name: &str,
    description: Option<&str>,
    version: Option<&str>,
    size: u64,
    icon_path: Option<&PathBuf>,
    icon_name: Option<&str>,
) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_selectable(false);
    row.set_activatable(true);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    line.add_css_class("app-listbox-item");
    line.append(&app_image(icon_path, icon_name, 45));
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let title = gtk::Label::builder()
        .label(name)
        .xalign(0.0)
        .css_classes(["heading"])
        .max_width_chars(70)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    text.append(&title);
    if let Some(description) = description.filter(|description| !description.is_empty()) {
        text.append(
            &gtk::Label::builder()
                .label(description)
                .xalign(0.0)
                .lines(1)
                .max_width_chars(100)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build(),
        );
    }
    let details = match version.filter(|version| !version.is_empty()) {
        Some(version) => format!("{} · {}", version, format_size(size)),
        None => format_size(size),
    };
    text.append(
        &gtk::Label::builder()
            .label(details)
            .xalign(0.0)
            .css_classes(["subtitle"])
            .margin_top(3)
            .build(),
    );
    line.append(&text);
    row.set_child(Some(&line));
    row
}
