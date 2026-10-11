//! Launch, removal and confirmation actions for integrated applications.

use super::*;

pub fn launch_current(state: &UiState) {
    if let Some(app) = state.current_detail.borrow().clone() {
        launch_app(&app, state);
    }
}

fn launch_app(app: &InstalledApp, state: &UiState) {
    let mut command = std::process::Command::new(&app.appimage_path);
    for (key, value) in &app.environment {
        command.env(key, value);
    }
    command.args(&app.exec_arguments);
    if let Err(error) = command.spawn() {
        toast(
            state,
            &i18n::tr("Unable to launch: {error}").replace("{error}", &error.to_string()),
        );
    }
}

pub fn remove_current(state: &UiState) {
    let Some(app) = state.current_detail.borrow().clone() else {
        return;
    };
    let dialog = adw::MessageDialog::builder()
        .transient_for(&state.window)
        .heading(i18n::tr("Do you really want to remove this app?"))
        .body(app.appimage_path.display().to_string())
        .build();
    dialog.add_response("cancel", &i18n::tr("Cancel"));
    dialog.add_response("remove", &i18n::tr("Remove"));
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    let state_clone = state.clone();
    dialog.connect_response(None, move |dialog, response| {
        if response == "remove" {
            let app = app.clone();
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let _ = sender.send(service::remove_app(&app));
            });
            let state = state_clone.clone();
            glib::timeout_add_local(std::time::Duration::from_millis(30), move || match receiver
                .try_recv()
            {
                Ok(result) => {
                    match result {
                        Ok(_) => {
                            toast(&state, &i18n::tr("AppImage removed"));
                            show_list(&state);
                        }
                        Err(error) => toast(&state, &error.to_string()),
                    };
                    glib::ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => glib::ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => glib::ControlFlow::Break,
            });
        }
        dialog.close();
    });
    dialog.present();
}
