//! Stable JSON output for the local `--list-installed --json` command.

use crate::model::InstalledApp;
use serde::Serialize;

#[derive(Serialize)]
struct InstalledDocument {
    schema_version: u32,
    installed: Vec<InstalledItem>,
}

#[derive(Serialize)]
struct InstalledItem {
    name: String,
    path: String,
    desktop_id: Option<String>,
    version: Option<String>,
}

pub fn print_installed(apps: &[InstalledApp]) {
    let document = InstalledDocument {
        schema_version: 1,
        installed: apps
            .iter()
            .map(|app| InstalledItem {
                name: app.name.clone(),
                path: app.appimage_path.to_string_lossy().into_owned(),
                desktop_id: app
                    .desktop_file_path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned()),
                version: app.version.clone(),
            })
            .collect(),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&document).unwrap_or_else(|_| "{}".into())
    );
}
