//! Desktop Entry parsing, AppImage discovery and desktop-file generation.
//!
//! Discovery intentionally starts from `applications/*.desktop`, matching
//! Gearlever's definition of an integrated app rather than scanning the storage
//! directory for arbitrary executables.

use crate::model::{InstalledApp, InstalledStatus};
use anyhow::{Context, Result, anyhow};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const APP_MANAGER_MARKER: &str = "appimanager";

#[derive(Debug, Clone, Default)]
pub struct DesktopEntry {
    pub values: HashMap<String, String>,
}

impl DesktopEntry {
    pub fn parse(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .with_context(|| format!("cannot read desktop file {}", path.display()))?;
        let mut values = HashMap::new();
        let mut in_desktop_entry = false;

        for line in content.lines() {
            let line = line.trim();
            if line == "[Desktop Entry]" {
                in_desktop_entry = true;
                continue;
            }
            if line.starts_with('[') {
                in_desktop_entry = false;
            }
            if !in_desktop_entry || line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                values.insert(key.trim().to_string(), unescape(value.trim()));
            }
        }

        if values.is_empty() {
            return Err(anyhow!("desktop file has no Desktop Entry section"));
        }
        Ok(Self { values })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
}

pub fn applications_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("applications")
}

pub fn icons_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("icons")
        .join("hicolor")
        .join("scalable")
        .join("apps")
}

pub fn parse_exec(command: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;

    for ch in command.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '\'' | '"' if quote == Some(ch) => quote = None,
            '\'' | '"' if quote.is_none() => quote = Some(ch),
            c if c.is_whitespace() && quote.is_none() => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            c => current.push(c),
        }
    }
    if escaped {
        current.push('\\');
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

pub fn desktop_exec(path: &Path, entry: &DesktopEntry) -> Vec<String> {
    let command = entry
        .get("Exec")
        .or_else(|| entry.get("TryExec"))
        .unwrap_or_default();
    let tokens = parse_exec(command);
    if tokens.is_empty() {
        return vec![path.to_string_lossy().into_owned()];
    }
    let mut tokens = tokens;
    if tokens.first().map(String::as_str) == Some("env") {
        tokens.remove(0);
        while tokens
            .first()
            .map(|value| value.contains('='))
            .unwrap_or(false)
        {
            tokens.remove(0);
        }
    }
    tokens
        .into_iter()
        .filter(|arg| !matches!(arg.as_str(), "%f" | "%F" | "%u" | "%U" | "%i" | "%c" | "%k"))
        .collect()
}

pub fn load_installed(default_directory: &Path, show_external: bool) -> Result<Vec<InstalledApp>> {
    let directory = applications_dir();
    if !directory.exists() {
        return Ok(Vec::new());
    }

    let mut apps = Vec::new();
    for item in
        fs::read_dir(&directory).with_context(|| format!("cannot scan {}", directory.display()))?
    {
        let item = item?;
        let path = item.path();
        if path.extension().and_then(|x| x.to_str()) != Some("desktop") {
            continue;
        }

        let entry = match DesktopEntry::parse(&path) {
            Ok(entry) => entry,
            Err(_) => continue,
        };
        if entry.get("Type") != Some("Application") {
            continue;
        }

        let executable = entry.get("TryExec").map(PathBuf::from).or_else(|| {
            desktop_exec(Path::new(""), &entry)
                .first()
                .map(PathBuf::from)
        });
        let Some(executable) = executable else {
            continue;
        };
        if !executable.is_file() || !crate::appimage::is_appimage(&executable) {
            continue;
        }

        let in_default = executable
            .parent()
            .map(|parent| parent == default_directory)
            .unwrap_or(false);
        if !in_default && !show_external {
            continue;
        }

        let metadata = match fs::metadata(&executable) {
            Ok(metadata) => metadata,
            Err(_) => continue,
        };
        let config = crate::config::load_app_config(&executable).unwrap_or_default();
        let icon_value = entry.get("Icon").map(str::to_string);
        let icon_path = icon_value.as_deref().and_then(|icon| {
            let path = PathBuf::from(icon);
            if path.is_file() { Some(path) } else { None }
        });
        let icon_name = icon_value.filter(|icon| !Path::new(icon).is_file());
        let app_type = crate::appimage::detect_type(&executable);
        let architecture = crate::appimage::detect_architecture(&executable);
        let name = entry
            .get("Name")
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| {
                executable
                    .file_stem()
                    .and_then(|x| x.to_str())
                    .unwrap_or("AppImage")
            })
            .to_string();

        apps.push(InstalledApp {
            name,
            description: entry.get("Comment").map(str::to_string),
            version: entry.get("X-AppImage-Version").map(str::to_string),
            appimage_path: executable,
            desktop_file_path: path,
            icon_path,
            icon_name,
            exec_arguments: config.arguments,
            environment: config.environment,
            architecture,
            appimage_type: app_type,
            size: metadata.len(),
            terminal: entry.get("Terminal") == Some("true"),
            external_folder: !in_default,
            status: InstalledStatus::Installed,
        });
    }

    apps.sort_by_key(|app| app.name.to_lowercase());
    Ok(apps)
}

pub fn quote_exec_arg(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || "@%+=:,./_-".contains(ch))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub fn write_desktop_file(
    path: &Path,
    candidate: &crate::model::ImportCandidate,
    appimage_path: &Path,
    icon_path: Option<&Path>,
) -> Result<()> {
    let exec = build_exec(
        appimage_path,
        &candidate.exec_arguments,
        &candidate.environment,
    );

    let icon = icon_path
        .map(|path| path.to_string_lossy().into_owned())
        .or_else(|| candidate.icon_name.clone())
        .unwrap_or_else(|| "application-x-executable".to_string());
    let mut content = String::from("[Desktop Entry]\n");
    content.push_str("Type=Application\n");
    content.push_str(&format!("Name={}\n", escape(&candidate.name)));
    if let Some(description) = &candidate.description {
        content.push_str(&format!("Comment={}\n", escape(description)));
    }
    content.push_str(&format!("Exec={}\n", exec));
    content.push_str(&format!(
        "TryExec={}\n",
        escape(&appimage_path.to_string_lossy())
    ));
    content.push_str(&format!("Icon={}\n", escape(&icon)));
    content.push_str(&format!(
        "Terminal={}\n",
        if candidate.terminal { "true" } else { "false" }
    ));
    content.push_str("Categories=Utility;\n");
    if let Some(version) = &candidate.version {
        content.push_str(&format!("X-AppImage-Version={}\n", escape(version)));
    }
    content.push_str(&format!("X-AppImage-Manager={}\n", APP_MANAGER_MARKER));
    content.push_str(&format!(
        "X-AppImage-Path={}\n",
        escape(&appimage_path.to_string_lossy())
    ));

    fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

pub fn write_desktop_exec(
    path: &Path,
    appimage_path: &Path,
    arguments: &[String],
    environment: &[(String, String)],
) -> Result<()> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("cannot read desktop file {}", path.display()))?;
    let replacement = format!("Exec={}", build_exec(appimage_path, arguments, environment));
    let mut output = Vec::new();
    let mut in_desktop_entry = false;
    let mut replaced = false;

    for line in content.lines() {
        if line == "[Desktop Entry]" {
            in_desktop_entry = true;
        } else if line.starts_with('[') {
            if in_desktop_entry && !replaced {
                output.push(replacement.clone());
                replaced = true;
            }
            in_desktop_entry = false;
        }

        if in_desktop_entry && line.starts_with("Exec=") {
            if !replaced {
                output.push(replacement.clone());
                replaced = true;
            }
        } else {
            output.push(line.to_string());
        }
    }
    if in_desktop_entry && !replaced {
        output.push(replacement);
    }
    fs::write(path, format!("{}\n", output.join("\n")))?;
    Ok(())
}

pub fn build_exec(
    appimage_path: &Path,
    arguments: &[String],
    environment: &[(String, String)],
) -> String {
    let mut exec = String::new();
    if !environment.is_empty() {
        exec.push_str("env ");
        for (key, value) in environment {
            exec.push_str(&quote_exec_arg(&format!("{}={}", key, value)));
            exec.push(' ');
        }
    }
    exec.push_str(&quote_exec_arg(&appimage_path.to_string_lossy()));
    for argument in arguments {
        exec.push(' ');
        exec.push_str(&quote_exec_arg(argument));
    }
    exec
}

fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(value: &str) -> String {
    let mut output = String::new();
    let mut escaped = false;
    for ch in value.chars() {
        if escaped {
            output.push(match ch {
                'n' => '\n',
                't' => '\t',
                's' => ' ',
                'r' => '\r',
                other => other,
            });
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else {
            output.push(ch);
        }
    }
    if escaped {
        output.push('\\');
    }
    output
}

pub fn app_id_for(name: &str, path: &Path) -> String {
    let mut id = String::new();
    for ch in name.chars().chain(path.to_string_lossy().chars()) {
        if ch.is_ascii_alphanumeric() {
            id.push(ch.to_ascii_lowercase());
        } else {
            id.push('-');
        }
    }
    while id.contains("--") {
        id = id.replace("--", "-");
    }
    id.trim_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_exec_arguments_and_environment_prefixes() {
        assert_eq!(
            parse_exec("env FOO=bar '/tmp/My App.AppImage' --name 'two words' %U"),
            vec![
                "env",
                "FOO=bar",
                "/tmp/My App.AppImage",
                "--name",
                "two words",
                "%U"
            ]
        );
        let entry = DesktopEntry {
            values: HashMap::from([(
                "Exec".to_string(),
                "env FOO=bar '/tmp/My App.AppImage' --name value".to_string(),
            )]),
        };
        assert_eq!(
            desktop_exec(Path::new(""), &entry)[0],
            "/tmp/My App.AppImage"
        );
    }

    #[test]
    fn builds_quoted_desktop_exec_without_a_shell() {
        let exec = build_exec(
            Path::new("/tmp/My App.AppImage"),
            &["--name".to_string(), "two words".to_string()],
            &[("APP_MODE".to_string(), "test".to_string())],
        );
        assert_eq!(
            exec,
            "env APP_MODE=test '/tmp/My App.AppImage' --name 'two words'"
        );
    }
}
