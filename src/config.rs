//! Persistent application and per-AppImage settings.
//!
//! Global preferences use Gio.Settings and the project's schema. A one-time
//! JSON import preserves settings created by earlier development builds.

use crate::model::{AppConfig, SettingsData};
use anyhow::Result;
use gio::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

fn config_directory() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("appimanager")
}

fn settings_path() -> PathBuf {
    config_directory().join("settings.json")
}

pub fn load_settings() -> SettingsData {
    let Some(settings) = gio_settings() else {
        return read_legacy_settings().unwrap_or_default();
    };

    // Earlier AppImage Manager builds stored global preferences in JSON. Import
    // that file once into the same Gio.Settings keys used by Gearlever.
    if !settings.boolean("legacy-config-migrated") {
        if let Some(legacy) = read_legacy_settings() {
            write_gio_settings(&settings, &legacy);
        }
        let _ = settings.set_boolean("legacy-config-migrated", true);
    }
    read_gio_settings(&settings).unwrap_or_default()
}

pub fn save_settings(settings: &SettingsData) -> Result<()> {
    if let Some(gio_settings) = gio_settings() {
        write_gio_settings(&gio_settings, settings);
        let _ = gio_settings.set_boolean("legacy-config-migrated", true);
    } else {
        fs::create_dir_all(config_directory())?;
        fs::write(settings_path(), serde_json::to_vec_pretty(settings)?)?;
    }
    Ok(())
}

fn read_legacy_settings() -> Option<SettingsData> {
    fs::read(settings_path())
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
}

fn gio_settings() -> Option<gio::Settings> {
    let default = gio::SettingsSchemaSource::default();
    let source = gio::SettingsSchemaSource::from_directory(
        env!("APPIMANAGER_SCHEMA_DIR"),
        default.as_ref(),
        true,
    )
    .ok()
    .or(default)?;
    let schema = source.lookup("io.github.appimanager.AppImageManager", false)?;
    Some(gio::Settings::new_full(
        &schema,
        None::<&gio::SettingsBackend>,
        None,
    ))
}

fn read_gio_settings(settings: &gio::Settings) -> Option<SettingsData> {
    let directory = settings.string("appimages-default-folder");
    let directory = directory.as_str();
    let directory = if let Some(relative) = directory.strip_prefix("~/") {
        dirs::home_dir()?.join(relative)
    } else if directory == "~" {
        dirs::home_dir()?
    } else {
        PathBuf::from(directory)
    };
    Some(SettingsData {
        appimages_directory: directory,
        show_external: settings.boolean("manage-files-outside-default-folder"),
        move_on_import: settings.boolean("move-appimage-on-integration"),
        use_safe_extraction: settings.boolean("block-unsafe-extractor"),
        minimal_preview: settings.boolean("preview-minimal-ui"),
        debug_logs: settings.boolean("debug-logs"),
        is_maximized: settings.boolean("is-maximized"),
        exec_as_name_for_terminal_apps: settings.boolean("exec-as-name-for-terminal-apps"),
    })
}

fn write_gio_settings(settings: &gio::Settings, data: &SettingsData) {
    let _ = settings.set_string(
        "appimages-default-folder",
        &data.appimages_directory.to_string_lossy(),
    );
    let _ = settings.set_boolean("manage-files-outside-default-folder", data.show_external);
    let _ = settings.set_boolean("move-appimage-on-integration", data.move_on_import);
    let _ = settings.set_boolean("block-unsafe-extractor", data.use_safe_extraction);
    let _ = settings.set_boolean("preview-minimal-ui", data.minimal_preview);
    let _ = settings.set_boolean("debug-logs", data.debug_logs);
    let _ = settings.set_boolean("is-maximized", data.is_maximized);
    let _ = settings.set_boolean(
        "exec-as-name-for-terminal-apps",
        data.exec_as_name_for_terminal_apps,
    );
}

fn app_config_path(appimage_path: &Path) -> PathBuf {
    let mut hasher = sha2::Sha256::new();
    use sha2::Digest;
    hasher.update(appimage_path.to_string_lossy().as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    config_directory()
        .join("apps")
        .join(format!("{}.json", &hash[..16]))
}

pub fn load_app_config(path: &Path) -> Result<AppConfig> {
    let path = app_config_path(path);
    if !path.exists() {
        return Ok(AppConfig::default());
    }
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn save_app_config(path: &Path, config: &AppConfig) -> Result<()> {
    let path = app_config_path(path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(config)?)?;
    Ok(())
}

pub fn remove_app_config(path: &Path) -> Result<()> {
    let path = app_config_path(path);
    if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}
