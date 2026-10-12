//! gettext initialization and small translation helpers.

use std::path::PathBuf;

pub fn init() {
    // Called at process entry, before GTK or any worker thread is started.
    unsafe {
        gettextrs::setlocale(gettextrs::LocaleCategory::LcAll, "");
    }
    let build_locale_directory = PathBuf::from(env!("OUT_DIR")).join("locale");
    let locale_directory = std::env::var_os("APPIMANAGER_LOCALEDIR")
        .map(PathBuf::from)
        .or_else(|| {
            build_locale_directory
                .is_dir()
                .then_some(build_locale_directory)
        })
        .unwrap_or_else(|| PathBuf::from("/usr/share/locale"));
    let _ = gettextrs::bindtextdomain("appimanager", &locale_directory);
    let _ = gettextrs::bind_textdomain_codeset("appimanager", "UTF-8");
    let _ = gettextrs::textdomain("appimanager");
}

pub fn tr(message: &str) -> String {
    gettextrs::gettext(message)
}
