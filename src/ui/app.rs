//! Libadwaita application lifecycle and top-level page composition.

use super::imports::prepare_minimal_preview;
use super::*;

pub fn run() {
    i18n::init();
    match cli::run_if_requested() {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("appimanager: {error:#}");
            std::process::exit(1);
        }
    }
    let app = adw::Application::new(Some(APP_ID), ApplicationFlags::HANDLES_OPEN);
    app.connect_startup(|app| {
        let _ = adw::init();
        let provider = gtk::CssProvider::new();
        // Keep overrides minimal; libadwaita owns the colors and typography.
        provider.load_from_data(".app-listbox-item { margin: 5px; } .big-btn { padding: 10px; }");
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        app.set_accels_for_action("app.import", &["<primary>o"]);
        app.set_accels_for_action("app.search", &["<primary>f"]);
    });
    let launch_has_files = std::env::args_os()
        .skip(1)
        .any(|argument| !argument.to_string_lossy().starts_with('-'));
    app.connect_activate(move |app| {
        if !launch_has_files {
            build_ui(app, false);
        }
    });
    app.connect_open(|app, files, _hint| {
        if application_state(app).is_none() {
            build_ui(app, true);
        }
        if let Some(state) = application_state(app) {
            state.opened_from_file.set(true);
            if state.settings.borrow().minimal_preview {
                state.stack.set_visible_child_name("opening");
                prepare_minimal_preview(&state);
            }
            let paths: Vec<PathBuf> = files.iter().filter_map(|file| file.path()).collect();
            if !paths.is_empty() {
                open_import_paths(&state, paths, true);
            } else {
                restore_file_preview(&state);
                show_list(&state);
                state.window.present();
            }
        }
    });
    add_actions(&app);
    app.run();
}

fn add_actions(app: &adw::Application) {
    let import = gio::SimpleAction::new("import", None);
    let app_clone = app.clone();
    import.connect_activate(move |_, _| {
        if let Some(state) = application_state(&app_clone) {
            choose_files(&state);
        }
    });
    app.add_action(&import);

    let search = gio::SimpleAction::new("search", None);
    let app_clone = app.clone();
    search.connect_activate(move |_, _| {
        if let Some(state) = application_state(&app_clone) {
            state.search_bar.set_search_mode(true);
            state.search.grab_focus();
        }
    });
    app.add_action(&search);

    let preferences = gio::SimpleAction::new("preferences", None);
    let app_clone = app.clone();
    preferences.connect_activate(move |_, _| {
        if let Some(state) = application_state(&app_clone) {
            show_settings(&state);
        }
    });
    app.add_action(&preferences);
}

thread_local! {
    static APP_STATES: RefCell<Vec<(String, UiState)>> = const { RefCell::new(Vec::new()) };
}

fn application_key(app: &adw::Application) -> String {
    app.application_id()
        .map(|id| id.to_string())
        .unwrap_or_else(|| APP_ID.to_string())
}

fn set_application_state(app: &adw::Application, state: &UiState) {
    let key = application_key(app);
    APP_STATES.with(|states| {
        let mut states = states.borrow_mut();
        states.retain(|(old_key, _)| old_key != &key);
        states.push((key, state.clone()));
    });
}

fn application_state(app: &adw::Application) -> Option<UiState> {
    let key = application_key(app);
    APP_STATES.with(|states| {
        states
            .borrow()
            .iter()
            .find(|(old_key, _)| old_key == &key)
            .map(|(_, state)| state.clone())
    })
}

pub fn add_stack_page(stack: &adw::Leaflet, child: &impl IsA<gtk::Widget>, name: &str) {
    stack.append(child).set_name(Some(name));
}

fn build_ui(app: &adw::Application, opened_from_file: bool) {
    if let Some(state) = application_state(app) {
        if state.opened_from_file.replace(false) {
            restore_file_preview(&state);
            show_list(&state);
        } else {
            refresh_apps(&state);
        }
        state.window.present();
        return;
    }

    let settings = Rc::new(RefCell::new(config::load_settings()));
    let apps = Rc::new(RefCell::new(Vec::new()));
    let current_detail = Rc::new(RefCell::new(None));
    let current_imports = Rc::new(RefCell::new(Vec::new()));
    let detail_is_candidate = Rc::new(Cell::new(false));
    let candidate_trusted = Rc::new(Cell::new(false));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("AppImage Manager")
        .default_width(660)
        .default_height(600)
        .build();
    let header = adw::HeaderBar::new();
    let views = adw::ViewStack::new();
    let switcher = adw::ViewSwitcherTitle::builder().stack(&views).build();
    // There is one installed-app tab after removing network updates: show an
    // actual window title instead of leaving an empty view switcher slot.
    switcher.set_title("AppImage Manager");
    switcher.set_view_switcher_enabled(false);
    header.set_title_widget(Some(&switcher));

    let import_button = gtk::Button::new();
    let import_content = adw::ButtonContent::new();
    import_content.set_icon_name("list-add-symbolic");
    import_content.set_label(&i18n::tr("Open"));
    import_button.set_child(Some(&import_content));
    import_button.set_tooltip_text(Some(&i18n::tr("Open a new AppImage")));
    header.pack_start(&import_button);

    let preferences_button = presentation::icon_button("settings", "Preferences");
    header.pack_end(&preferences_button);

    let search = gtk::SearchEntry::builder()
        .placeholder_text(i18n::tr("Filter installed applications"))
        .hexpand(true)
        .build();
    let search_bar = gtk::SearchBar::new();
    search_bar.set_child(Some(&search));
    search_bar.connect_entry(&search);
    search_bar.set_key_capture_widget(Some(&window));
    let search_button = gtk::ToggleButton::new();
    search_button.set_child(Some(&presentation::icon("search", 18)));
    search_button.set_tooltip_text(Some(&i18n::tr("Search")));
    header.pack_end(&search_button);

    let stack = adw::Leaflet::builder()
        .can_unfold(false)
        .can_navigate_back(true)
        .build();
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&stack));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&overlay));
    window.set_content(Some(&toolbar));

    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_activate_on_single_click(true);
    list.add_css_class("boxed-list");
    let (empty_page, empty_open_preferences) = build_empty_page();
    let list_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let search_clamp = adw::Clamp::builder()
        .maximum_size(600)
        .child(&search_bar)
        .build();
    list_page.append(&search_clamp);
    let list_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_body.add_css_class("page-content");
    list_body.set_margin_top(20);
    list_body.set_margin_bottom(20);
    let list_heading = gtk::Label::builder()
        .label(i18n::tr("Installed applications"))
        .css_classes(["title-2"])
        .xalign(0.0)
        .margin_bottom(15)
        .build();
    list_body.append(&list_heading);
    list_body.append(&list);
    let list_clamp = adw::Clamp::builder()
        .maximum_size(600)
        .child(&list_body)
        .build();
    let list_content = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::None)
        .vexpand(true)
        .build();
    list_content.add_named(&list_clamp, Some("applications"));
    list_content.add_named(&empty_page, Some("empty"));
    // Start with the empty page; the async scan switches to the installed list
    // only after rows exist, avoiding a brief clipped heading at the top.
    list_content.set_visible_child_name("empty");
    let list_scroll = gtk::ScrolledWindow::builder()
        .child(&list_content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    list_page.append(&list_scroll);
    let installed = views.add_titled(&list_page, Some("installed"), &i18n::tr("Installed"));
    installed.set_icon_name(Some("computer-symbolic"));
    add_stack_page(&stack, &views, "list");

    let detail_page = build_details_page();
    add_stack_page(&stack, &detail_page.page, "details");
    let import_page = build_import_page();
    add_stack_page(&stack, &import_page.page, "import");
    let opening_page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    opening_page.set_halign(gtk::Align::Center);
    opening_page.set_valign(gtk::Align::Center);
    let opening_spinner = gtk::Spinner::new();
    opening_spinner.start();
    opening_page.append(&opening_spinner);
    opening_page.append(&gtk::Label::new(Some(&i18n::tr("Opening AppImage…"))));
    add_stack_page(&stack, &opening_page, "opening");

    let state = UiState {
        window: window.clone(),
        overlay: overlay.clone(),
        stack: stack.clone(),
        list: list.clone(),
        list_content: list_content.clone(),
        search: search.clone(),
        search_bar: search_bar.clone(),
        title: detail_page.title.clone(),
        subtitle: detail_page.subtitle.clone(),
        detail_icon: detail_page.icon.clone(),
        detail_info: detail_page.info.clone(),
        detail_actions: detail_page.actions.clone(),
        detail_banner: detail_page.banner.clone(),
        detail_pills: detail_page.pills.clone(),
        detail_arguments: detail_page.arguments.clone(),
        detail_environment: detail_page.environment.clone(),
        detail_launch: detail_page.launch.clone(),
        detail_primary: detail_page.remove.clone(),
        detail_is_candidate,
        detail_website: detail_page.website.clone(),
        detail_website_row: detail_page.website_row.clone(),
        detail_reload: detail_page.reload_metadata.clone(),
        detail_advanced: detail_page.advanced.clone(),
        detail_reload_row: detail_page.reload_row.clone(),
        import_button: import_button.clone(),
        preferences_button: preferences_button.clone(),
        search_button: search_button.clone(),
        opened_from_file: Rc::new(Cell::new(opened_from_file)),
        import_list: import_page.list.clone(),
        import_warning: import_page.already_installed.clone(),
        current_detail,
        current_imports,
        candidate_trusted,
        apps,
        settings,
    };
    set_application_state(app, &state);

    let state_clone = state.clone();
    import_button.connect_clicked(move |_| {
        if state_clone.stack.visible_child_name().as_deref() == Some("list") {
            choose_files(&state_clone);
        } else {
            show_list(&state_clone);
        }
    });
    let state_clone = state.clone();
    list.connect_row_activated(move |_, row| {
        let query = state_clone.search.text().to_lowercase().replace(' ', "");
        let app = state_clone
            .apps
            .borrow()
            .iter()
            .filter(|app| query.is_empty() || app.name.to_lowercase().contains(&query))
            .nth(row.index().max(0) as usize)
            .cloned();
        if let Some(app) = app {
            state_clone.search_bar.set_search_mode(false);
            show_details(&state_clone, app);
        }
    });
    let state_clone = state.clone();
    empty_open_preferences.connect_clicked(move |_| show_settings(&state_clone));
    let state_clone = state.clone();
    search.connect_search_changed(move |_| filter_rows(&state_clone, ""));
    let search_bar_clone = search_bar.clone();
    let search_for_button = search.clone();
    search_button.connect_toggled(move |button| {
        search_bar_clone.set_search_mode(button.is_active());
        if button.is_active() {
            search_for_button.grab_focus();
        }
    });
    let search_button_clone = search_button.clone();
    let search_clone = search.clone();
    let state_for_search = state.clone();
    search_bar.connect_search_mode_enabled_notify(move |bar| {
        search_button_clone.set_active(bar.is_search_mode());
        if !bar.is_search_mode() {
            search_clone.set_text("");
            render_apps(&state_for_search);
        }
    });
    let import_content_page = import_content.clone();
    let search_button_page = search_button.clone();
    let import_button_page = import_button.clone();
    stack.connect_visible_child_name_notify(move |visible_stack| {
        let page_name = visible_stack.visible_child_name();
        let in_list = page_name.as_deref() == Some("list");
        search_button_page.set_visible(in_list);
        import_content_page.set_icon_name(if in_list {
            "list-add-symbolic"
        } else {
            "go-previous-symbolic"
        });
        let label = if in_list {
            i18n::tr("Open")
        } else {
            String::new()
        };
        import_content_page.set_label(&label);
        import_button_page.set_tooltip_text(Some(&i18n::tr(if in_list {
            "Open a new AppImage"
        } else {
            "Go back"
        })));
    });

    let state_clone = state.clone();
    preferences_button.connect_clicked(move |_| show_settings(&state_clone));
    let state_clone = state.clone();
    import_page
        .install_all
        .connect_clicked(move |_| install_current(&state_clone));
    let state_clone = state.clone();
    detail_page
        .launch
        .connect_clicked(move |_| launch_current(&state_clone));
    let state_clone = state.clone();
    detail_page.remove.connect_clicked(move |_| {
        if state_clone.detail_is_candidate.get() {
            install_current(&state_clone);
        } else {
            remove_current(&state_clone);
        }
    });
    let state_clone = state.clone();
    detail_page
        .save_environment
        .connect_clicked(move |_| save_current_details(&state_clone));
    let state_clone = state.clone();
    detail_page
        .open_website
        .connect_clicked(move |_| open_current_website(&state_clone));
    let state_clone = state.clone();
    detail_page
        .website
        .connect_activate(move |_| save_website(&state_clone));
    let state_clone = state.clone();
    let website_focus = gtk::EventControllerFocus::new();
    website_focus.connect_leave(move |_| save_website_if_valid(&state_clone));
    detail_page.website.add_controller(website_focus);
    let state_clone = state.clone();
    detail_page
        .reload_metadata
        .connect_clicked(move |_| reload_current_metadata(&state_clone));
    let state_clone = state.clone();
    detail_page
        .banner
        .connect_button_clicked(move |_| unlock_candidate(&state_clone));

    install_drop_target(&state);
    if opened_from_file && state.settings.borrow().minimal_preview {
        state.stack.set_visible_child_name("opening");
        prepare_minimal_preview(&state);
    }
    if state.settings.borrow().is_maximized && !opened_from_file {
        window.maximize();
    }
    let state_for_maximize = state.clone();
    window.connect_maximized_notify(move |window| {
        if !state_for_maximize.opened_from_file.get() {
            state_for_maximize.settings.borrow_mut().is_maximized = window.is_maximized();
            let _ = config::save_settings(&state_for_maximize.settings.borrow());
        }
    });
    window.present();
    refresh_apps(&state);
}

pub(super) fn restore_file_preview(state: &UiState) {
    state.detail_info.set_visible(true);
    state.detail_pills.set_visible(true);
    state.detail_advanced.set_visible(true);
    state.detail_reload_row.set_visible(true);
    state.detail_website_row.set_visible(true);
    state
        .detail_actions
        .set_orientation(gtk::Orientation::Horizontal);
    state.import_button.set_visible(true);
    state.preferences_button.set_visible(true);
    state.search_button.set_visible(true);
    state.window.set_resizable(true);
    state.window.set_default_size(660, 600);
    if state.settings.borrow().is_maximized {
        state.window.maximize();
    }
}

fn build_empty_page() -> (adw::StatusPage, gtk::Button) {
    let hint = gtk::Label::new(Some(&i18n::tr(
        "If you don't see your apps, check if the selected folder is correct.",
    )));
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.set_hexpand(true);
    hint.set_max_width_chars(52);
    let open_preferences = gtk::Button::with_label(&i18n::tr("Open Preferences"));
    open_preferences.add_css_class("pill");
    open_preferences.set_halign(gtk::Align::Center);
    let actions = gtk::Box::new(gtk::Orientation::Vertical, 14);
    actions.set_hexpand(true);
    actions.append(&hint);
    actions.append(&open_preferences);
    let page = adw::StatusPage::builder()
        .icon_name("document-open-symbolic")
        .title(i18n::tr("Get started"))
        .description(i18n::tr(
            "Drag and drop an AppImage here or click on the \"+\" icon.",
        ))
        .child(&actions)
        .build();
    (page, open_preferences)
}
