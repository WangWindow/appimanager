//! Application details using libadwaita's content width and banner patterns.

use super::*;

pub struct DetailPage {
    pub page: gtk::ScrolledWindow,
    pub title: gtk::Label,
    pub subtitle: gtk::Label,
    pub icon: gtk::Image,
    pub info: gtk::Box,
    pub actions: gtk::Box,
    pub banner: adw::Banner,
    pub pills: adw::ToggleGroup,
    pub arguments: adw::EntryRow,
    pub environment: environment::EnvironmentEditor,
    pub save_environment: gtk::Button,
    pub launch: gtk::Button,
    pub remove: gtk::Button,
    pub website: adw::EntryRow,
    pub website_row: gtk::Box,
    pub open_website: gtk::Button,
    pub reload_metadata: gtk::Button,
    pub advanced: gtk::Box,
    pub reload_row: gtk::ListBox,
}

pub fn build_details_page() -> DetailPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 20);
    content.add_css_class("page-content");
    content.set_margin_top(10);
    content.set_margin_bottom(10);
    content.set_margin_start(20);
    content.set_margin_end(20);

    let icon = presentation::icon("app-window", 96);
    icon.set_halign(gtk::Align::Center);
    let title = gtk::Label::builder()
        .css_classes(["title-1"])
        .wrap(true)
        .halign(gtk::Align::Center)
        .hexpand(true)
        .build();
    let subtitle = gtk::Label::builder()
        .css_classes(["dim-label"])
        .halign(gtk::Align::Center)
        .max_width_chars(60)
        .lines(2)
        .selectable(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .justify(gtk::Justification::Center)
        .build();
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 4);
    titles.append(&title);
    titles.append(&subtitle);

    let pills = adw::ToggleGroup::builder()
        .can_target(false)
        .css_classes(["round"])
        .halign(gtk::Align::Center)
        .margin_top(20)
        .margin_bottom(10)
        .build();

    let launch = gtk::Button::with_label(&i18n::tr("Launch"));
    let remove = gtk::Button::with_label(&i18n::tr("Remove"));
    for button in [&launch, &remove] {
        button.set_css_classes(&["big-btn", "text-button"]);
        button.set_width_request(200);
        button.set_valign(gtk::Align::Center);
        button.set_halign(gtk::Align::Center);
    }
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_halign(gtk::Align::Center);
    actions.set_margin_top(20);
    actions.append(&launch);
    actions.append(&remove);

    let banner = adw::Banner::new("");

    let info = gtk::Box::new(gtk::Orientation::Vertical, 20);

    let website = adw::EntryRow::builder()
        .title(i18n::tr("Add a website"))
        .build();
    website.add_prefix(&presentation::icon("globe", 18));
    let open_website = presentation::icon_button("external-link", "Open URL");
    open_website.set_valign(gtk::Align::Center);
    website.add_suffix(&open_website);
    let website_list = gtk::ListBox::new();
    website_list.add_css_class("boxed-list");
    website_list.set_selection_mode(gtk::SelectionMode::None);
    website_list.append(&website);
    let website_row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    website_row.append(&website_list);

    let advanced = gtk::Box::new(gtk::Orientation::Vertical, 16);
    advanced.add_css_class("settings-section");
    let reload_metadata = gtk::Button::with_label(&i18n::tr("Reload metadata"));
    let reload_row = gtk::ListBox::new();
    reload_row.add_css_class("boxed-list");
    reload_row.set_selection_mode(gtk::SelectionMode::None);
    let reload_list_row = adw::ActionRow::builder()
        .title(i18n::tr("Reload metadata"))
        .subtitle(i18n::tr("Update information like icon, version and description.\nUseful if the app updated itself."))
        .selectable(false)
        .build();
    reload_list_row.add_prefix(&presentation::icon("refresh-cw", 18));
    reload_metadata.set_valign(gtk::Align::Center);
    reload_list_row.add_suffix(&reload_metadata);
    reload_row.append(&reload_list_row);

    let arguments = adw::EntryRow::builder()
        .title(i18n::tr("Command line arguments"))
        .build();
    arguments.add_prefix(&presentation::icon("app-window", 18));
    let arguments_row = gtk::ListBox::new();
    arguments_row.add_css_class("boxed-list");
    arguments_row.set_selection_mode(gtk::SelectionMode::None);
    arguments_row.append(&arguments);

    let save_environment = gtk::Button::with_label(&i18n::tr("Save"));
    save_environment.add_css_class("suggested-action");
    save_environment.set_sensitive(false);
    let environment = environment::EnvironmentEditor::new(&save_environment);
    let environment_heading = gtk::Label::builder()
        .label(i18n::tr("Environment variables"))
        .xalign(0.0)
        .css_classes(["heading"])
        .build();
    let add_environment = presentation::icon_button("plus", "Add environment variable");
    let editor = environment.clone();
    add_environment.connect_clicked(move |_| editor.add("", ""));
    let env_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    env_actions.append(&add_environment);
    env_actions.append(&save_environment);
    let environment_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    environment_header.append(&environment_heading);
    environment_header.append(&gtk::Box::builder().hexpand(true).build());
    environment_header.append(&env_actions);

    advanced.append(&reload_row);
    advanced.append(&arguments_row);
    advanced.append(&environment_header);
    advanced.append(&environment.widget);

    content.append(&icon);
    content.append(&titles);
    content.append(&pills);
    content.append(&actions);
    content.append(&info);
    content.append(&website_row);
    content.append(&advanced);

    let clamp = adw::Clamp::builder()
        .child(&content)
        .maximum_size(600)
        .margin_top(10)
        .margin_bottom(20)
        .build();
    let page_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page_content.append(&banner);
    page_content.append(&clamp);
    let page = gtk::ScrolledWindow::builder()
        .child(&page_content)
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    DetailPage {
        page,
        title,
        subtitle,
        icon,
        info,
        actions,
        banner,
        pills,
        arguments,
        environment,
        save_environment,
        launch,
        remove,
        website,
        website_row,
        open_website,
        reload_metadata,
        advanced,
        reload_row,
    }
}
