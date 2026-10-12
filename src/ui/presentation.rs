//! Reusable, purely presentational GTK helpers.

use super::*;

/// Uses GTK symbolic icons for consistent sizing across the application.
pub fn icon(name: &str, pixel_size: i32) -> gtk::Image {
    let icon_name = match name {
        "app-window" => "application-x-executable-symbolic",
        "arrow-left" => "go-previous-symbolic",
        "arrow-right" => "go-next-symbolic",
        "check" => "emblem-ok-symbolic",
        "circle-alert" => "dialog-warning-symbolic",
        "external-link" => "external-link-symbolic",
        "file-plus" => "document-new-symbolic",
        "folder" => "folder-symbolic",
        "globe" => "web-browser-symbolic",
        "hash" => "emblem-documents-symbolic",
        "info" => "dialog-information-symbolic",
        "menu" => "open-menu-symbolic",
        "plus" => "list-add-symbolic",
        "refresh-cw" => "view-refresh-symbolic",
        "search" => "system-search-symbolic",
        "settings" => "preferences-system-symbolic",
        "trash" => "user-trash-symbolic",
        _ => "image-missing-symbolic",
    };
    let image = gtk::Image::from_icon_name(icon_name);
    image.set_pixel_size(pixel_size);
    image
}

pub fn icon_button(name: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_child(Some(&icon(name, 18)));
    if !tooltip.is_empty() {
        button.set_tooltip_text(Some(&i18n::tr(tooltip)));
    }
    button
}

pub fn app_image(path: Option<&PathBuf>, icon_name: Option<&str>, pixel_size: i32) -> gtk::Image {
    let image = gtk::Image::new();
    set_app_image(&image, path, icon_name, pixel_size);
    image
}

pub fn set_app_image(
    image: &gtk::Image,
    path: Option<&PathBuf>,
    icon_name: Option<&str>,
    pixel_size: i32,
) {
    if let Some(path) = path.filter(|path| path.is_file())
        && let Ok(pixbuf) =
            gtk::gdk_pixbuf::Pixbuf::from_file_at_scale(path, pixel_size, pixel_size, true)
    {
        image.set_from_pixbuf(Some(&pixbuf));
    } else if let Some(icon_name) = icon_name {
        image.set_icon_name(Some(icon_name));
    } else {
        image.set_icon_name(Some("application-x-executable-symbolic"));
    }
    image.set_pixel_size(pixel_size);
}

/// Formats bytes with the same GNU-style compact unit labels used by Gearlever.
pub fn format_size(size: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = size as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", size, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}
