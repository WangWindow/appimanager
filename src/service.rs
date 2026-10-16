//! Shared application services used by both GTK actions and CLI commands.
//!
//! Functions in this module do not create or inspect GTK widgets. Slow file IO
//! and integration changes can therefore run on worker threads.

use crate::appimage;
use crate::config;
use crate::desktop;
use crate::model::{AppConfig, ImportCandidate, InstalledApp, InstalledStatus, SettingsData};
use anyhow::{Context, Result, anyhow};
use gio::prelude::*;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn install_candidate(candidate: &ImportCandidate, settings: &SettingsData) -> Result<()> {
    install_candidate_with_policy(candidate, settings, false)
}

pub fn keep_both_candidate(candidate: &ImportCandidate, settings: &SettingsData) -> Result<()> {
    install_candidate_with_policy(candidate, settings, true)
}

fn install_candidate_with_policy(
    candidate: &ImportCandidate,
    settings: &SettingsData,
    force_keep_both: bool,
) -> Result<()> {
    fs::create_dir_all(&settings.appimages_directory)?;
    let preferred = appimage::preferred_destination(&settings.appimages_directory, &candidate.name);
    let (destination, display_name) = if force_keep_both || preferred.exists() {
        let version = version_label(candidate);
        (
            appimage::keep_both_destination(
                &settings.appimages_directory,
                &candidate.name,
                &version,
            ),
            format!("{} ({version})", candidate.name),
        )
    } else {
        (preferred, candidate.name.clone())
    };

    let source_is_already_managed =
        candidate.path.parent() == Some(settings.appimages_directory.as_path());
    if settings.move_on_import && !source_is_already_managed {
        fs::rename(&candidate.path, &destination).or_else(|_| {
            fs::copy(&candidate.path, &destination).map(|_| ())?;
            fs::remove_file(&candidate.path)
        })?;
    } else {
        fs::copy(&candidate.path, &destination)?;
    }
    make_executable(&destination)?;

    let app_id = desktop::app_id_for(&candidate.name, &destination);
    let desktop_path = desktop::applications_dir().join(format!("{app_id}.desktop"));
    let _ = write_integration(
        candidate,
        &destination,
        &desktop_path,
        &display_name,
        AppConfig {
            arguments: candidate.exec_arguments.clone(),
            environment: candidate.environment.clone(),
            ..AppConfig::default()
        },
    )?;
    Ok(())
}

/// Replaces one existing integration in place, retaining its desktop file and
/// the launch settings the user configured for it.
pub fn replace_candidate(
    candidate: &ImportCandidate,
    settings: &SettingsData,
    existing: &InstalledApp,
) -> Result<()> {
    fs::create_dir_all(&settings.appimages_directory)?;
    let destination = replacement_destination(candidate, settings, existing);
    let mut updated_candidate = candidate.clone();
    updated_candidate.exec_arguments = existing.exec_arguments.clone();
    updated_candidate.environment = existing.environment.clone();

    copy_atomically(&candidate.path, &destination)?;
    make_executable(&destination)?;

    let mut app_config = config::load_app_config(&existing.appimage_path).unwrap_or_default();
    app_config.arguments = existing.exec_arguments.clone();
    app_config.environment = existing.environment.clone();
    let new_icon = write_integration(
        &updated_candidate,
        &destination,
        &existing.desktop_file_path,
        &candidate.name,
        app_config,
    )?;

    if let Some(old_icon) = &existing.icon_path
        && new_icon.as_ref() != Some(old_icon)
    {
        // The new desktop entry already points at the replacement icon (or at
        // the generic fallback), so the old extracted icon is no longer used.
        let _ = fs::remove_file(old_icon);
    }

    if existing.appimage_path != destination {
        config::remove_app_config(&existing.appimage_path)?;
    }
    let source_is_already_managed =
        candidate.path.parent() == Some(settings.appimages_directory.as_path());
    if settings.move_on_import
        && !source_is_already_managed
        && candidate.path != destination
        && candidate.path.exists()
    {
        fs::remove_file(&candidate.path).with_context(|| {
            format!(
                "cannot move {} into the AppImages folder",
                candidate.path.display()
            )
        })?;
    }
    Ok(())
}

fn replacement_destination(
    candidate: &ImportCandidate,
    settings: &SettingsData,
    existing: &InstalledApp,
) -> PathBuf {
    if !existing.external_folder {
        return existing.appimage_path.clone();
    }

    let file_name = existing
        .appimage_path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| {
            appimage::preferred_destination(&settings.appimages_directory, &candidate.name)
                .file_name()
                .unwrap_or_default()
                .to_os_string()
        });
    let destination = settings.appimages_directory.join(file_name);
    if destination.exists() {
        appimage::unique_destination(&settings.appimages_directory, &candidate.name)
    } else {
        destination
    }
}

fn version_label(candidate: &ImportCandidate) -> String {
    candidate
        .version
        .as_deref()
        .filter(|version| !version.trim().is_empty())
        .map(str::to_string)
        .or_else(|| candidate.hashes.sha256.get(..8).map(str::to_string))
        .unwrap_or_else(|| "unknown".to_string())
}

fn write_integration(
    candidate: &ImportCandidate,
    destination: &Path,
    desktop_path: &Path,
    display_name: &str,
    app_config: AppConfig,
) -> Result<Option<PathBuf>> {
    let icon = candidate
        .icon_path
        .as_ref()
        .map(|path| appimage::copy_icon(path, destination, desktop_path))
        .transpose()?;
    if let Some(parent) = desktop_path.parent() {
        fs::create_dir_all(parent)?;
    }
    desktop::write_desktop_file_with_name(
        desktop_path,
        candidate,
        destination,
        icon.as_deref(),
        display_name,
    )?;
    config::save_app_config(destination, &app_config)?;
    Ok(icon)
}

fn copy_atomically(source: &Path, destination: &Path) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| anyhow!("AppImage destination has no parent folder"))?;
    fs::create_dir_all(parent)?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("appimage");

    for attempt in 0..100 {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let staging = parent.join(format!(
            ".{name}.tmp-{}-{timestamp}-{attempt}",
            std::process::id()
        ));
        let mut output = match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
        {
            Ok(output) => output,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        };
        let result = (|| -> Result<()> {
            let mut input = File::open(source)
                .with_context(|| format!("cannot read AppImage {}", source.display()))?;
            io::copy(&mut input, &mut output)?;
            output.sync_all()?;
            make_executable(&staging)?;
            fs::rename(&staging, destination)
                .with_context(|| format!("cannot replace AppImage at {}", destination.display()))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&staging);
        }
        return result;
    }

    Err(anyhow!(
        "cannot create a temporary AppImage beside the destination"
    ))
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
    let icon = candidate
        .icon_path
        .as_ref()
        .map(|path| appimage::copy_icon(path, &app.appimage_path, &app.desktop_file_path))
        .transpose()?;
    desktop::write_desktop_file_with_name(
        &app.desktop_file_path,
        &candidate,
        &app.appimage_path,
        icon.as_deref(),
        &app.name,
    )?;
    let mut app_config = config::load_app_config(&app.appimage_path).unwrap_or_default();
    app_config.arguments = app.exec_arguments.clone();
    app_config.environment = app.environment.clone();
    config::save_app_config(&app.appimage_path, &app_config)?;
    Ok(InstalledApp {
        name: app.name.clone(),
        identity_name: candidate.name,
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
