//! Preferences follow Gearlever's libadwaita groups and rows, without its
//! removed network-update settings.

use super::*;

fn switch(title: &str, subtitle: &str, active: bool) -> adw::SwitchRow {
    let row = adw::SwitchRow::builder()
        .title(i18n::tr(title))
        .active(active)
        .build();
    // gettext("") returns the catalog header, not an empty subtitle.
    if !subtitle.is_empty() {
        row.set_subtitle(&i18n::tr(subtitle));
    }
    row
}

pub fn show_settings(state: &UiState) {
    let window = adw::PreferencesWindow::builder()
        .transient_for(&state.window)
        .title(i18n::tr("Preferences"))
        .modal(true)
        .default_width(540)
        .default_height(640)
        .build();
    let page = adw::PreferencesPage::new();
    let settings = state.settings.borrow().clone();

    let general = adw::PreferencesGroup::builder()
        .title(i18n::tr("General"))
        .build();
    let directory = adw::ActionRow::builder()
        .title(i18n::tr("AppImage default location"))
        .subtitle(settings.appimages_directory.to_string_lossy())
        .build();
    let choose = presentation::icon_button("folder", "Select a folder");
    choose.set_valign(gtk::Align::Center);
    directory.add_suffix(&choose);
    general.add(&directory);
    let external = switch(
        "Show integrated AppImages outside the default folder",
        "List AppImages that have been integrated into the system menu but are located outside the default folder",
        settings.show_external,
    );
    let preview = switch(
        "Preview new apps in a minimal UI",
        "",
        settings.minimal_preview,
    );
    let safe = switch(
        "Use safe mechanisms to load metadata",
        "Read metadata without running the AppImage. Turn off only for trusted files that cannot be parsed safely.",
        settings.use_safe_extraction,
    );
    general.add(&external);
    general.add(&preview);
    general.add(&safe);

    let files = adw::PreferencesGroup::builder()
        .title(i18n::tr("File management"))
        .build();
    let move_row = adw::ActionRow::builder()
        .title(i18n::tr("Move AppImages into the destination folder"))
        .subtitle(i18n::tr("Reduce disk usage"))
        .build();
    let move_check = gtk::CheckButton::builder()
        .valign(gtk::Align::Center)
        .active(settings.move_on_import)
        .build();
    move_row.add_prefix(&move_check);
    let copy_row = adw::ActionRow::builder()
        .title(i18n::tr("Clone AppImages into the destination folder"))
        .subtitle(i18n::tr(
            "Keep the original file and create a copy in the destination folder",
        ))
        .build();
    let copy_check = gtk::CheckButton::builder()
        .group(&move_check)
        .valign(gtk::Align::Center)
        .active(!settings.move_on_import)
        .build();
    copy_row.add_prefix(&copy_check);
    files.add(&move_row);
    files.add(&copy_row);

    let naming = adw::PreferencesGroup::builder()
        .title(i18n::tr("Naming conventions"))
        .build();
    let terminal = switch(
        "Use executable name for integrated terminal apps",
        "Use the executable name for terminal apps. Add the destination folder to PATH manually.",
        settings.exec_as_name_for_terminal_apps,
    );
    naming.add(&terminal);

    let debugging = adw::PreferencesGroup::builder()
        .title(i18n::tr("Debugging"))
        .build();
    let debug = switch(
        "Enable debug logs",
        "Uses more disk space. Restart required.",
        settings.debug_logs,
    );
    debugging.add(&debug);
    for group in [&general, &files, &naming, &debugging] {
        page.add(group);
    }
    window.add(&page);

    bind_switch(state, &external, |s, value| s.show_external = value, true);
    bind_switch(state, &preview, |s, value| s.minimal_preview = value, false);
    bind_switch(
        state,
        &safe,
        |s, value| s.use_safe_extraction = value,
        false,
    );
    bind_switch(
        state,
        &terminal,
        |s, value| s.exec_as_name_for_terminal_apps = value,
        false,
    );
    bind_switch(state, &debug, |s, value| s.debug_logs = value, false);
    let state_clone = state.clone();
    move_check.connect_toggled(move |check| {
        state_clone.settings.borrow_mut().move_on_import = check.is_active();
        if let Err(error) = config::save_settings(&state_clone.settings.borrow()) {
            show_message(&state_clone, &i18n::tr("Unknown Error"), &error.to_string());
        }
    });
    let state_clone = state.clone();
    choose.connect_clicked(move |_| choose_directory(&state_clone, &directory));
    window.present();
}

fn bind_switch(
    state: &UiState,
    row: &adw::SwitchRow,
    set: fn(&mut SettingsData, bool),
    refresh: bool,
) {
    let state = state.clone();
    row.connect_active_notify(move |row| {
        set(&mut state.settings.borrow_mut(), row.is_active());
        if let Err(error) = config::save_settings(&state.settings.borrow()) {
            show_message(&state, &i18n::tr("Unknown Error"), &error.to_string());
        }
        if refresh {
            refresh_apps(&state);
        }
    });
}

pub fn choose_directory(state: &UiState, row: &adw::ActionRow) {
    let dialog = gtk::FileDialog::builder()
        .title(i18n::tr("Select a folder"))
        .modal(true)
        .build();
    let state_clone = state.clone();
    let row = row.clone();
    dialog.select_folder(
        Some(&state.window),
        None::<&gio::Cancellable>,
        move |result| {
            if let Ok(file) = result
                && let Some(path) = file.path()
            {
                state_clone.settings.borrow_mut().appimages_directory = path.clone();
                row.set_subtitle(&path.to_string_lossy());
                if let Err(error) = config::save_settings(&state_clone.settings.borrow()) {
                    show_message(&state_clone, &i18n::tr("Unknown Error"), &error.to_string());
                }
                refresh_apps(&state_clone);
            }
        },
    );
}
