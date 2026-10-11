//! Shared application services used by both GTK actions and CLI commands.
//!
//! Functions in this module do not create or inspect GTK widgets. Slow file IO
//! and integration changes can therefore run on worker threads.

use crate::appimage;
use crate::config;
use crate::desktop;
use crate::model::{AppConfig, ImportCandidate, InstalledApp, InstalledStatus, SettingsData};
use anyhow::Result;
use gio::prelude::*;
use std::fs;
use std::path::Path;

pub fn install_candidate(candidate: &ImportCandidate, settings: &SettingsData) -> Result<()> {
    fs::create_dir_all(&settings.appimages_directory)?;
    let destination = appimage::unique_destination(&settings.appimages_directory, &candidate.name);
    if settings.move_on_import {
        fs::rename(&candidate.path, &destination).or_else(|_| {
            fs::copy(&candidate.path, &destination).map(|_| ())?;
            fs::remove_file(&candidate.path)
        })?;
    } else {
        fs::copy(&candidate.path, &destination)?;
    }
    make_executable(&destination)?;
    let app_id = desktop::app_id_for(&candidate.name, &destination);
    let icon = candidate
        .icon_path
        .as_ref()
        .map(|path| appimage::copy_icon(path, &app_id))
        .transpose()?;
    let desktop_dir = desktop::applications_dir();
    fs::create_dir_all(&desktop_dir)?;
    let desktop_path = desktop_dir.join(format!("{}.desktop", app_id));
    desktop::write_desktop_file(&desktop_path, candidate, &destination, icon.as_deref())?;
    config::save_app_config(
        &destination,
        &AppConfig {
            arguments: candidate.exec_arguments.clone(),
            environment: candidate.environment.clone(),
            ..AppConfig::default()
        },
    )?;
    Ok(())
}

pub fn remove_app(app: &InstalledApp) -> Result<()> {
    remove_app_with_mode(app, false)
}

pub fn remove_app_with_mode(app: &InstalledApp, permanent: bool) -> Result<()> {
    if app.appimage_path.exists() {
        let file = gio::File::for_path(&app.appimage_path);
        if permanent || file.trash(None::<&gio::Cancellable>).is_err() {
            fs::remove_file(&app.appimage_path)?;
        }
    }
    if app.desktop_file_path.exists() {
        fs::remove_file(&app.desktop_file_path)?;
    }
    if let Some(icon) = &app.icon_path {
        let _ = fs::remove_file(icon);
    }
    config::remove_app_config(&app.appimage_path)?;
    Ok(())
}

pub fn reload_metadata(app: &InstalledApp, settings: &SettingsData) -> Result<InstalledApp> {
    let mut candidate = appimage::inspect(&app.appimage_path, !settings.use_safe_extraction)?;
    candidate.exec_arguments = app.exec_arguments.clone();
    candidate.environment = app.environment.clone();
    let app_id = desktop::app_id_for(&app.name, &app.appimage_path);
    let icon = candidate
        .icon_path
        .as_ref()
        .map(|path| appimage::copy_icon(path, &app_id))
        .transpose()?;
    desktop::write_desktop_file(
        &app.desktop_file_path,
        &candidate,
        &app.appimage_path,
        icon.as_deref(),
    )?;
    let mut app_config = config::load_app_config(&app.appimage_path).unwrap_or_default();
    app_config.arguments = app.exec_arguments.clone();
    app_config.environment = app.environment.clone();
    config::save_app_config(&app.appimage_path, &app_config)?;
    Ok(InstalledApp {
        name: candidate.name,
        description: candidate.description,
        version: candidate.version,
        appimage_path: app.appimage_path.clone(),
        desktop_file_path: app.desktop_file_path.clone(),
        icon_path: icon,
        icon_name: candidate.icon_name,
        exec_arguments: app.exec_arguments.clone(),
        environment: app.environment.clone(),
        architecture: candidate.architecture,
        appimage_type: candidate.appimage_type,
        size: fs::metadata(&app.appimage_path)?.len(),
        terminal: candidate.terminal,
        external_folder: app.external_folder,
        status: InstalledStatus::Installed,
    })
}

fn make_executable(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}
