//! GTK user interface, split into focused components and controllers.
//!
//! The `app` module composes the main window; page implementations live next
//! to their controllers so widget layout and behavior can be reviewed together.

#![allow(deprecated)]

mod actions;
mod app;
mod details;
mod details_controller;
mod dialogs;
mod environment;
mod imports;
mod list;
mod preferences;
mod presentation;
mod state;

pub use actions::{launch_current, remove_current};
pub use app::{add_stack_page, run};
pub use details::{DetailPage, build_details_page};
pub use details_controller::{
    filter_rows, open_current_website, reload_current_metadata, save_current_details, save_website,
    save_website_if_valid, show_details,
};
pub use dialogs::{install_drop_target, show_message, toast};
pub use environment::EnvironmentEditor;
pub use imports::{
    ImportPage, build_import_page, choose_files, install_current, open_import_paths,
    show_candidate, unlock_candidate,
};
pub use list::{app_row, refresh_apps, render_apps, show_list};
pub use preferences::{choose_directory, show_settings};
pub use presentation::{app_image, format_size};
pub use state::UiState;

use crate::{appimage, cli, config, desktop, i18n, model, service};
use adw::prelude::*;
use gio::ApplicationFlags;
use gio::prelude::*;
use gtk4 as gtk;
use libadwaita as adw;
use model::{Architecture, ImportCandidate, InstalledApp, SettingsData};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;

pub const APP_ID: &str = "io.github.appimanager";
