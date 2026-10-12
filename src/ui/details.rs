//! Application details using libadwaita's content width and banner patterns.

use super::*;

pub struct DetailPage {
    pub page: gtk::ScrolledWindow,
    pub content: gtk::Box,
    pub clamp: adw::Clamp,
    pub title: gtk::Label,
    pub subtitle: gtk::Label,
    pub icon: gtk::Image,
    pub info: gtk::Box,
    pub actions: gtk::Box,
    pub banner: adw::Banner,
    pub pills: adw::ToggleGroup,
    pub arguments: gtk::Entry,
    pub arguments_display: gtk::Label,
    pub edit_arguments: gtk::Button,
    pub environment: environment::EnvironmentEditor,
    pub save_environment: gtk::Button,
    pub launch: gtk::Button,
    pub remove: gtk::Button,
    pub website: gtk::Entry,
    pub website_display: gtk::Label,
    pub edit_website: gtk::Button,
    pub website_row: gtk::Box,
    pub open_website: gtk::Button,
    pub reload_metadata: gtk::Button,
    pub advanced: gtk::Box,
    pub reload_row: gtk::ListBox,
}

pub fn build_details_page() -> DetailPage {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 20);
    content.add_css_class("page-content");
    content.set_margin_top(0);
    content.set_margin_bottom(0);
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
    actions.append(&launch);
    actions.append(&remove);

    let banner = adw::Banner::new("");

    let info = gtk::Box::new(gtk::Orientation::Vertical, 20);

    let (website_list, website_content) = presentation::boxed_icon_row("globe");
    let website_display = gtk::Label::builder()
        .label(i18n::tr("Add a website"))
        .hexpand(true)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    website_display.add_css_class("dim-label");
    let website = gtk::Entry::builder()
        .placeholder_text(i18n::tr("Add a website"))
        .hexpand(true)
        .build();
    website.set_visible(false);
    let edit_website = presentation::icon_button("edit", "Edit");
    edit_website.set_valign(gtk::Align::Center);
    let open_website = presentation::icon_button("external-link", "Open URL");
    open_website.set_valign(gtk::Align::Center);
    open_website.set_visible(false);
    website_content.append(&website_display);
    website_content.append(&website);
    website_content.append(&presentation::widget_slot(
        Some(open_website.clone().upcast()),
        44,
    ));
    website_content.append(&presentation::widget_slot(
        Some(edit_website.clone().upcast()),
        44,
    ));
    let website_row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    website_row.append(&website_list);

    let advanced = gtk::Box::new(gtk::Orientation::Vertical, 16);
    advanced.add_css_class("settings-section");
    let reload_metadata = gtk::Button::with_label(&i18n::tr("Reload metadata"));
    let (reload_row, reload_content) = presentation::boxed_icon_row("refresh-cw");
    let reload_info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    reload_info.set_hexpand(true);
    let reload_title = gtk::Label::builder()
        .label(i18n::tr("Reload metadata"))
        .xalign(0.0)
        .build();
    let reload_subtitle = gtk::Label::builder()
        .label(i18n::tr(
            "Update information like icon, version and description.\nUseful if the app updated itself.",
        ))
        .xalign(0.0)
        .wrap(true)
        .css_classes(["dim-label"])
        .build();
    reload_info.append(&reload_title);
    reload_info.append(&reload_subtitle);
    reload_metadata.set_valign(gtk::Align::Center);
    reload_content.append(&reload_info);
    reload_content.append(&presentation::widget_slot(None, 24));
    reload_content.append(&presentation::widget_slot(
        Some(reload_metadata.clone().upcast()),
        44,
    ));

    let (arguments_row, arguments_content) = presentation::boxed_icon_row("app-window");
    let arguments_display = gtk::Label::builder()
        .label(i18n::tr("Command line arguments"))
        .hexpand(true)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    arguments_display.add_css_class("dim-label");
    let arguments = gtk::Entry::builder()
        .placeholder_text(i18n::tr("Command line arguments"))
        .hexpand(true)
        .build();
    arguments.set_visible(false);
    let edit_arguments = presentation::icon_button("edit", "Edit");
    edit_arguments.set_valign(gtk::Align::Center);
    arguments_content.append(&arguments_display);
    arguments_content.append(&arguments);
    arguments_content.append(&presentation::widget_slot(
        Some(edit_arguments.clone().upcast()),
        44,
    ));

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
        content,
        clamp,
        title,
        subtitle,
        icon,
        info,
        actions,
        banner,
        pills,
        arguments,
        arguments_display,
        edit_arguments,
        environment,
        save_environment,
        launch,
        remove,
        website,
        website_display,
        edit_website,
        website_row,
        open_website,
        reload_metadata,
        advanced,
        reload_row,
    }
}
