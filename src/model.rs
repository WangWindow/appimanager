//! Domain types shared between desktop discovery, integration and UI.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Architecture {
    X86_64,
    Aarch64,
    Arm,
    Unknown,
}

impl Architecture {
    pub fn current() -> Self {
        match std::env::consts::ARCH {
            "x86_64" => Self::X86_64,
            "aarch64" => Self::Aarch64,
            "arm" | "armv7" => Self::Arm,
            _ => Self::Unknown,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Arm => "arm",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppImageType {
    Type1,
    Type2,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstalledStatus {
    NotInstalled,
    Installed,
    Installing,
    Uninstalling,
    Error,
    Unknown,
}

impl AppImageType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Type1 => "Type 1",
            Self::Type2 => "Type 2",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone)]
pub struct InstalledApp {
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub appimage_path: PathBuf,
    pub desktop_file_path: PathBuf,
    pub icon_path: Option<PathBuf>,
    pub icon_name: Option<String>,
    pub exec_arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub architecture: Architecture,
    pub appimage_type: AppImageType,
    pub size: u64,
    pub terminal: bool,
    pub external_folder: bool,
    pub status: InstalledStatus,
}

#[derive(Debug, Clone)]
pub struct ImportCandidate {
    pub path: PathBuf,
    pub name: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub icon_path: Option<PathBuf>,
    pub icon_name: Option<String>,
    pub architecture: Architecture,
    pub appimage_type: AppImageType,
    pub size: u64,
    pub terminal: bool,
    pub exec_arguments: Vec<String>,
    pub environment: Vec<(String, String)>,
    pub trusted: bool,
    pub hashes: AppImageHashes,
}

/// File digests displayed in the AppImage preview, computed in one read pass.
#[derive(Debug, Clone, Default)]
pub struct AppImageHashes {
    pub md5: String,
    pub sha1: String,
    pub sha256: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub environment: Vec<(String, String)>,
    #[serde(default)]
    pub website: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SettingsData {
    pub appimages_directory: PathBuf,
    pub show_external: bool,
    pub move_on_import: bool,
    pub use_safe_extraction: bool,
    pub minimal_preview: bool,
    pub debug_logs: bool,
    pub is_maximized: bool,
    pub exec_as_name_for_terminal_apps: bool,
}

impl Default for SettingsData {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        Self {
            appimages_directory: home.join("AppImages"),
            show_external: true,
            move_on_import: true,
            use_safe_extraction: true,
            minimal_preview: true,
            debug_logs: false,
            is_maximized: false,
            exec_as_name_for_terminal_apps: false,
        }
    }
}
