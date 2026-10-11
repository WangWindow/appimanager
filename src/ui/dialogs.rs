//! Application-level dialogs, logging entry point and drag/drop feedback.

use super::*;

pub fn install_drop_target(state: &UiState) {
    let drag_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    drag_page.set_halign(gtk::Align::Center);
    drag_page.set_valign(gtk::Align::Center);
    let icon = gtk::Image::from_icon_name("document-open-symbolic");
    icon.set_pixel_size(120);
    icon.set_margin_bottom(30);
    drag_page.append(&icon);
    let label = gtk::Label::new(Some(&i18n::tr("Drop your file here")));
    label.add_css_class("title-2");
    label.set_margin_bottom(30);
    drag_page.append(&label);
    add_stack_page(&state.stack, &drag_page, "drag-drop");
    let target = gtk::DropTarget::new(
        gtk::gdk::FileList::static_type(),
        gtk::gdk::DragAction::COPY,
    );
    let state_clone = state.clone();
    target.connect_enter(move |_, _, _| {
        state_clone.stack.set_visible_child_name("drag-drop");
        gtk::gdk::DragAction::COPY
    });
    let state_clone = state.clone();
    target.connect_leave(move |_| {
        if state_clone.stack.visible_child_name().as_deref() == Some("drag-drop") {
            state_clone.stack.set_visible_child_name("list");
        }
    });
    let state_clone = state.clone();
    target.connect_drop(move |_, value, _, _| {
        let Ok(file_list) = value.get::<gtk::gdk::FileList>() else {
            return false;
        };
        let paths: Vec<PathBuf> = file_list
            .files()
            .into_iter()
            .filter_map(|file| file.path())
            .collect();
        if paths.is_empty() {
            return false;
        }
        open_import_paths(&state_clone, paths);
        true
    });
    state.stack.add_controller(target);
}

pub fn toast(state: &UiState, message: &str) {
    state.overlay.add_toast(adw::Toast::new(message));
}

pub fn show_message(state: &UiState, heading: &str, body: &str) {
    let dialog = adw::MessageDialog::builder()
        .transient_for(&state.window)
        .heading(heading)
        .body(body)
        .build();
    dialog.add_response("close", &i18n::tr("Close"));
    dialog.present();
}
