//! Shared, main-thread-only state for the GTK application.

use super::environment::EnvironmentEditor;
use crate::model::{ImportCandidate, InstalledApp, SettingsData};
use gtk4 as gtk;
use libadwaita as adw;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Clone)]
pub struct UiState {
    pub window: adw::ApplicationWindow,
    pub overlay: adw::ToastOverlay,
    pub stack: adw::Leaflet,
    pub list: gtk::ListBox,
    pub list_content: gtk::Stack,
    pub search: gtk::SearchEntry,
    pub search_bar: gtk::SearchBar,
    pub title: gtk::Label,
    pub subtitle: gtk::Label,
    pub detail_icon: gtk::Image,
    pub detail_content: gtk::Box,
    pub detail_clamp: adw::Clamp,
    pub detail_info: gtk::Box,
    pub detail_actions: gtk::Box,
    pub detail_banner: adw::Banner,
    pub detail_pills: adw::ToggleGroup,
    pub detail_arguments: gtk::Entry,
    pub detail_arguments_display: gtk::Label,
    pub edit_arguments_button: gtk::Button,
    pub detail_environment: EnvironmentEditor,
    pub detail_save_button: gtk::Button,
    pub detail_launch: gtk::Button,
    pub detail_primary: gtk::Button,
    pub detail_is_candidate: Rc<Cell<bool>>,
    pub detail_website: gtk::Entry,
    pub detail_website_display: gtk::Label,
    pub edit_website_button: gtk::Button,
    pub detail_open_website: gtk::Button,
    pub detail_website_row: gtk::Box,
    pub detail_reload: gtk::Button,
    pub detail_advanced: gtk::Box,
    pub detail_reload_row: gtk::ListBox,
    pub import_button: gtk::Button,
    pub preferences_button: gtk::Button,
    pub search_button: gtk::ToggleButton,
    pub import_list: gtk::ListBox,
    pub import_warning: gtk::Button,
    pub current_detail: Rc<RefCell<Option<InstalledApp>>>,
    pub current_imports: Rc<RefCell<Vec<ImportCandidate>>>,
    pub candidate_trusted: Rc<Cell<bool>>,
    pub opened_from_file: Rc<Cell<bool>>,
    pub apps: Rc<RefCell<Vec<InstalledApp>>>,
    pub settings: Rc<RefCell<SettingsData>>,
}
