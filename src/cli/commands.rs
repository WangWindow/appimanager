//! Local CLI command implementations shared with the GTK application services.

use super::Arguments;
use super::output;
use crate::appimage;
use crate::config;
use crate::desktop;
use crate::model::{InstalledApp, SettingsData};
use crate::service;
use anyhow::{Result, anyhow};
use std::io::{self, Write};
use std::path::Path;

pub fn run(arguments: &Arguments) -> Result<()> {
    let settings = config::load_settings();
    let installed = || desktop::load_installed(&settings.appimages_directory, true);

    if arguments.list_installed {
        return list_installed(arguments, &settings);
    }
    if arguments.remove_all {
        return remove_all(arguments, &installed()?);
    }
    if let Some(path) = &arguments.remove {
        return remove(arguments, find_app(&installed()?, path)?);
    }
    if let Some(path) = &arguments.integrate {
        return integrate(arguments, path, &settings, &installed()?);
    }
    Ok(())
}

fn list_installed(arguments: &Arguments, settings: &SettingsData) -> Result<()> {
    let apps = desktop::load_installed(&settings.appimages_directory, true)?;
    if arguments.json {
        output::print_installed(&apps);
    } else {
        for app in apps {
            println!("{}\t{}", app.name, app.appimage_path.display());
        }
    }
    Ok(())
}

fn remove_all(arguments: &Arguments, apps: &[InstalledApp]) -> Result<()> {
    if !confirm_if_needed(arguments, "Remove all integrated AppImages?")? {
        return Ok(());
    }
    for app in apps {
        service::remove_app_with_mode(app, arguments.delete)?;
        println!("{} was removed", app.appimage_path.display());
    }
    Ok(())
}

fn remove(arguments: &Arguments, app: InstalledApp) -> Result<()> {
    if !confirm_if_needed(arguments, &format!("Remove {}?", app.name))? {
        return Ok(());
    }
    service::remove_app_with_mode(&app, arguments.delete)?;
    println!("{} was removed", app.appimage_path.display());
    Ok(())
}

fn integrate(
    arguments: &Arguments,
    path: &Path,
    settings: &SettingsData,
    installed: &[InstalledApp],
) -> Result<()> {
    let mut candidate = appimage::inspect(path, !settings.use_safe_extraction)?;
    candidate.trusted = true;
    if !confirm_if_needed(arguments, &format!("Integrate {}?", candidate.name))? {
        return Ok(());
    }
    if arguments.replace {
        for existing in installed.iter().filter(|app| app.name == candidate.name) {
            service::remove_app(existing)?;
        }
    }
    service::install_candidate(&candidate, settings)?;
    println!("{} was integrated", path.display());
    Ok(())
}

fn find_app(apps: &[InstalledApp], path: &Path) -> Result<InstalledApp> {
    apps.iter()
        .find(|app| app.appimage_path == path || app.desktop_file_path == path)
        .cloned()
        .ok_or_else(|| anyhow!("AppImage is not integrated: {}", path.display()))
}

fn confirm_if_needed(arguments: &Arguments, question: &str) -> Result<bool> {
    if arguments.assume_yes {
        return Ok(true);
    }
    print!("{question} [y/N] ");
    io::stdout().flush()?;
    let mut answer = String::new();
    io::stdin().read_line(&mut answer)?;
    Ok(matches!(
        answer.trim().to_ascii_lowercase().as_str(),
        "y" | "yes"
    ))
}
