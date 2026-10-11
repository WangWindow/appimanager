//! gettext initialization and small translation helpers.

use std::path::PathBuf;

pub fn init() {
    // Called at process entry, before GTK or any worker thread is started.
    unsafe {
        gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");
    }
    let locale_directory = std::env::var_os("APPIMANAGER_LOCALEDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("OUT_DIR")).join("locale"));
    let _ = gettextrs::bindtextdomain("appimanager", &locale_directory);
    let _ = gettextrs::bind_textdomain_codeset("appimanager", "UTF-8");
    let _ = gettextrs::textdomain("appimanager");
}

pub fn tr(message: &str) -> String {
    gettextrs::gettext(message)
}
