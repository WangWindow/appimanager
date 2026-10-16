//! AppImage inspection, metadata extraction and import preparation.
//!
//! Two kinds of work happen here:
//!
//! * cheap header probing (magic bytes and ELF machine) that only reads the
//!   first 20 bytes of a file;
//! * metadata extraction that pulls `.desktop`, icon and `.DirIcon` assets out
//!   of the embedded SquashFS via the pure-Rust `appimage-rs` reader.
//!
//! Extracted assets are cached under `$XDG_CACHE_HOME/appimanager/extracted`
//! and keyed by path, file size and modification time, which is far cheaper than
//! hashing multi-hundred-megabyte AppImages while still invalidating when the
//! file is replaced.

use crate::desktop;
use crate::model::{AppImageHashes, AppImageType, Architecture, ImportCandidate};
use anyhow::{Context, Result, anyhow};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static UNIQUE_SUFFIX: AtomicU64 = AtomicU64::new(0);

/// Header information of a type 1/2 AppImage ELF.
#[derive(Debug, Clone, Copy)]
pub struct Probe {
    pub kind: AppImageType,
    pub architecture: Architecture,
}

/// Reads the ELF header once and derives both the AppImage type and the target
/// architecture, so callers do not open the file repeatedly.
pub fn probe(path: &Path) -> Option<Probe> {
    let mut file = fs::File::open(path).ok()?;
    let mut header = [0u8; 20];
    file.read_exact(&mut header).ok()?;
    if &header[0..4] != b"\x7fELF" {
        return None;
    }
    let kind = match &header[8..11] {
        b"AI\x01" => AppImageType::Type1,
        b"AI\x02" => AppImageType::Type2,
        _ => AppImageType::Unknown,
    };
    let machine = if header[5] == 2 {
        u16::from_be_bytes([header[18], header[19]])
    } else {
        u16::from_le_bytes([header[18], header[19]])
    };
    let architecture = match machine {
        40 => Architecture::Arm,
        62 => Architecture::X86_64,
        183 => Architecture::Aarch64,
        _ => Architecture::Unknown,
    };
    Some(Probe { kind, architecture })
}

pub fn is_appimage(path: &Path) -> bool {
    path.is_file() && probe(path).is_some_and(|probe| probe.kind != AppImageType::Unknown)
}

pub fn detect_type(path: &Path) -> AppImageType {
    probe(path)
        .map(|probe| probe.kind)
        .unwrap_or(AppImageType::Unknown)
}

pub fn detect_architecture(path: &Path) -> Architecture {
    probe(path)
        .map(|probe| probe.architecture)
        .unwrap_or(Architecture::Unknown)
}

/// Inspects an AppImage and returns everything needed for an import preview.
///
/// Metadata comes from the embedded `.desktop` file when the SquashFS payload
/// can be read; otherwise the file name is used and the remaining fields stay
/// empty so the caller can still integrate the file.
pub fn inspect(path: &Path, allow_unsafe_extraction: bool) -> Result<ImportCandidate> {
    let mut candidate = inspect_metadata(path, allow_unsafe_extraction)?;
    candidate.hashes = cached_hashes(path)?;
    Ok(candidate)
}

/// Reads preview metadata without hashing the full AppImage file.
///
/// The compact file-open preview does not display hashes, so it can skip the
/// potentially expensive full-file read and show the app as soon as metadata
/// extraction finishes.
pub fn inspect_metadata(path: &Path, allow_unsafe_extraction: bool) -> Result<ImportCandidate> {
    let Some(probe) = probe(path).filter(|probe| probe.kind != AppImageType::Unknown) else {
        return Err(anyhow!("the selected file is not an AppImage"));
    };
    let metadata = fs::metadata(path)?;
    let fallback_name = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("AppImage")
        .to_string();
    let mut candidate = ImportCandidate {
        path: path.to_path_buf(),
        name: fallback_name,
        description: None,
        version: None,
        icon_path: None,
        icon_name: None,
        architecture: probe.architecture,
        appimage_type: probe.kind,
        size: metadata.len(),
        terminal: false,
        exec_arguments: Vec::new(),
        environment: Vec::new(),
        trusted: false,
        hashes: Default::default(),
    };

    if let Some(extracted) = extract_metadata(path, allow_unsafe_extraction)? {
        apply_desktop_metadata(&mut candidate, &extracted);
    }

    Ok(candidate)
}

/// Calculates the three display hashes in one sequential file pass.
fn calculate_hashes(path: &Path) -> Result<AppImageHashes> {
    let mut file = fs::File::open(path)?;
    let mut md5 = Md5::new();
    let mut sha1 = Sha1::new();
    let mut sha256 = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        let chunk = &buffer[..count];
        md5.update(chunk);
        sha1.update(chunk);
        sha256.update(chunk);
    }
    Ok(AppImageHashes {
        md5: format!("{:x}", md5.finalize()),
        sha1: format!("{:x}", sha1.finalize()),
        sha256: format!("{:x}", sha256.finalize()),
    })
}

/// Reuses display hashes for an unchanged AppImage, so revisiting an import
/// preview does not add another full-file read after the first import.
fn cached_hashes(path: &Path) -> Result<AppImageHashes> {
    let key = cache_key(path)?;
    let directory = cache_root().with_file_name("hashes");
    let cached = directory.join(format!("{key}.txt"));
    if let Ok(contents) = fs::read_to_string(&cached) {
        let mut hashes = contents.lines();
        if let (Some(md5), Some(sha1), Some(sha256), None) =
            (hashes.next(), hashes.next(), hashes.next(), hashes.next())
        {
            return Ok(AppImageHashes {
                md5: md5.to_string(),
                sha1: sha1.to_string(),
                sha256: sha256.to_string(),
            });
        }
    }

    let hashes = calculate_hashes(path)?;
    if fs::create_dir_all(&directory).is_ok() {
        let temporary = directory.join(format!("{key}-{}.tmp", now_suffix()));
        let contents = format!("{}\n{}\n{}\n", hashes.md5, hashes.sha1, hashes.sha256);
        if fs::write(&temporary, contents).is_err() || fs::rename(&temporary, &cached).is_err() {
            let _ = fs::remove_file(temporary);
        }
    }
    Ok(hashes)
}

/// Reads `Name`, `Comment`, version, terminal flag, arguments and icon from the
/// extracted `.desktop` file.
fn apply_desktop_metadata(candidate: &mut ImportCandidate, extracted: &Path) {
    let Some(desktop_path) = find_desktop_file(extracted) else {
        candidate.icon_path = find_default_icon(extracted);
        return;
    };
    let Ok(entry) = desktop::DesktopEntry::parse(&desktop_path) else {
        candidate.icon_path = find_default_icon(extracted);
        return;
    };
    if let Some(name) = entry.get("Name").filter(|name| !name.is_empty()) {
        candidate.name = name.to_string();
    }
    candidate.description = entry.get("Comment").map(str::to_string);
    candidate.version = entry.get("X-AppImage-Version").map(str::to_string);
    candidate.terminal = entry.get("Terminal") == Some("true");
    let command = desktop::parse_exec(entry.get("Exec").unwrap_or_default());
    if command.len() > 1 {
        candidate.exec_arguments = command
            .into_iter()
            .skip(1)
            .filter(|argument| !argument.starts_with('%'))
            .collect();
    }
    if let Some(icon) = entry.get("Icon") {
        candidate.icon_path = find_icon(extracted, icon);
        if candidate.icon_path.is_none() && !Path::new(icon).is_file() {
            candidate.icon_name = Some(icon.to_string());
        }
    }
    if candidate.icon_path.is_none() {
        candidate.icon_path = find_default_icon(extracted);
    }
}

fn cache_root() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("appimanager")
        .join("extracted")
}

/// Cache key based on the identity of the file rather than its contents.
///
/// Hashing a full AppImage on every import costs hundreds of megabytes of IO;
/// A stable SHA-256 of the path, size and nanosecond mtime identifies a cache
/// entry without reading the AppImage payload. It is an identity key, not a
/// content-integrity check.
fn cache_key(path: &Path) -> Result<String> {
    let metadata = fs::metadata(path)?;
    let mtime = metadata
        .modified()
        .unwrap_or(UNIX_EPOCH)
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let identity = format!("{}\0{}\0{}", path.to_string_lossy(), metadata.len(), mtime);
    Ok(format!("{:x}", Sha256::digest(identity.as_bytes())))
}

/// Extracts metadata assets into the cache, falling back to the AppImage's own
/// `--appimage-extract` when allowed and the pure-Rust reader cannot parse it.
fn extract_metadata(path: &Path, allow_unsafe: bool) -> Result<Option<PathBuf>> {
    let destination = cache_root().join(cache_key(path)?);
    if destination.exists() {
        return Ok(Some(destination));
    }
    fs::create_dir_all(&destination)?;

    if extract_with_appimage_rs(path, &destination).unwrap_or(false) {
        return Ok(Some(destination));
    }

    if allow_unsafe && extract_with_appimage_binary(path, &destination)? {
        return Ok(Some(destination));
    }

    let _ = fs::remove_dir_all(&destination);
    Ok(None)
}

/// Extracts only the assets used by desktop integration. The embedded SquashFS
/// is read at its ELF offset, so no temporary copy of the AppImage is needed.
fn extract_with_appimage_rs(path: &Path, destination: &Path) -> Result<bool> {
    let image = ::appimage::AppImage::open(path).map_err(|error| anyhow!(error))?;
    // Match root-level and nested AppDir metadata explicitly; glob libraries
    // differ on whether `**/` also includes files directly at the root.
    unpack_patterns(&image, destination, &["*.desktop", "**/*.desktop"])?;
    unpack_patterns(&image, destination, &[".DirIcon", "**/.DirIcon"])?;

    let Some(desktop_path) = find_desktop_file(destination) else {
        return Ok(find_default_icon(destination).is_some());
    };
    let icon_name = desktop::DesktopEntry::parse(&desktop_path)
        .ok()
        .and_then(|entry| entry.get("Icon").map(str::to_string));
    if let Some(icon_name) = icon_name.as_deref() {
        unpack_named_icon(&image, destination, icon_name)?;
    }
    if icon_name
        .as_deref()
        .is_none_or(|icon| find_icon(destination, icon).is_none())
        && find_default_icon(destination).is_none()
    {
        // Some AppImages have a case-mismatched desktop Icon name or use an
        // uncommon extension. Extract image assets only as a fallback.
        unpack_all_icon_files(&image, destination)?;
    }
    Ok(true)
}

fn unpack_named_icon(image: &::appimage::AppImage, destination: &Path, icon: &str) -> Result<()> {
    let basename = Path::new(icon)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(icon);
    if Path::new(basename).extension().is_some() {
        let nested = format!("**/{basename}");
        unpack_patterns(image, destination, &[basename, &nested])?;
    } else {
        for extension in ICON_EXTENSIONS {
            let filename = format!("{basename}.{extension}");
            let nested = format!("**/{filename}");
            unpack_patterns(image, destination, &[&filename, &nested])?;
        }
    }
    Ok(())
}

const ICON_EXTENSIONS: [&str; 7] = ["svg", "png", "webp", "xpm", "ico", "jpg", "jpeg"];

fn unpack_all_icon_files(image: &::appimage::AppImage, destination: &Path) -> Result<()> {
    for extension in ICON_EXTENSIONS {
        let root_pattern = format!("*.{extension}");
        let nested_pattern = format!("**/*.{extension}");
        unpack_patterns(image, destination, &[&root_pattern, &nested_pattern])?;
    }
    Ok(())
}

fn unpack_patterns(
    image: &::appimage::AppImage,
    destination: &Path,
    patterns: &[&str],
) -> Result<()> {
    for pattern in patterns {
        image
            .unpack()
            .matching(*pattern)
            .replacing()
            .to(destination)
            .map_err(|error| anyhow!(error))?;
    }
    Ok(())
}

/// Last-resort fallback that runs the AppImage's own extractor. Only used when
/// the user disabled the safe extraction preference.
fn extract_with_appimage_binary(path: &Path, destination: &Path) -> Result<bool> {
    let copied = destination.join("appimage");
    fs::copy(path, &copied).with_context(|| format!("cannot copy {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = fs::metadata(&copied)?.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        fs::set_permissions(&copied, permissions)?;
    }
    let status = Command::new(&copied)
        .arg("--appimage-extract")
        .current_dir(destination)
        .status();
    let _ = fs::remove_file(&copied);
    Ok(status.map(|status| status.success()).unwrap_or(false))
}

fn find_desktop_file(root: &Path) -> Option<PathBuf> {
    let mut pending = std::collections::VecDeque::from([root.to_path_buf()]);
    while let Some(directory) = pending.pop_front() {
        let Ok(read_dir) = fs::read_dir(directory) else {
            continue;
        };
        let mut entries = read_dir
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();

        if let Some(path) = entries.iter().find(|path| {
            path.is_file()
                && path.extension().and_then(|extension| extension.to_str()) == Some("desktop")
        }) {
            return Some(path.clone());
        }
        pending.extend(entries.into_iter().filter(|path| path.is_dir()));
    }
    None
}

fn find_icon(root: &Path, icon: &str) -> Option<PathBuf> {
    find_dir_icon(root).or_else(|| find_named_icon(root, icon))
}

fn find_named_icon(root: &Path, icon: &str) -> Option<PathBuf> {
    let basename = Path::new(icon)
        .file_name()
        .and_then(|value| value.to_str())?;
    let (stem, original_extension) = match Path::new(basename)
        .extension()
        .and_then(|value| value.to_str())
    {
        Some(extension)
            if extension.eq_ignore_ascii_case("svg") || extension.eq_ignore_ascii_case("png") =>
        {
            (Path::new(basename).file_stem()?.to_str()?, Some(extension))
        }
        Some(extension) => (Path::new(basename).file_stem()?.to_str()?, Some(extension)),
        None => (basename, None),
    };

    // Gear Lever prefers a .DirIcon first, then root-level SVG/PNG assets,
    // followed by the conventional hicolor application icon sizes.
    let mut names = vec![format!("{stem}.svg"), format!("{stem}.png")];
    if let Some(extension) = original_extension
        && !extension.eq_ignore_ascii_case("svg")
        && !extension.eq_ignore_ascii_case("png")
    {
        names.insert(0, basename.to_string());
    }
    for name in &names {
        let path = root.join(name);
        if path.is_file() {
            return Some(path);
        }
    }

    let hicolor = root.join("usr/share/icons/hicolor");
    for relative in [
        format!("scalable/apps/{stem}.svg"),
        format!("512x512/apps/{stem}.png"),
        format!("256x256/apps/{stem}.png"),
        format!("128x128/apps/{stem}.png"),
        format!("96x96/apps/{stem}.png"),
    ] {
        let path = hicolor.join(relative);
        if path.is_file() {
            return Some(path);
        }
    }

    // Some AppImages use a non-standard icon directory. Keep a deterministic
    // basename fallback after the Gear Lever paths have been checked.
    let mut pending = std::collections::VecDeque::from([root.to_path_buf()]);
    while let Some(directory) = pending.pop_front() {
        let mut entries = fs::read_dir(directory)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .collect::<Vec<_>>();
        entries.sort();
        if let Some(path) = entries.iter().find(|path| {
            path.is_file()
                && path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| {
                        names.iter().any(|wanted| wanted.eq_ignore_ascii_case(name))
                    })
        }) {
            return Some(path.clone());
        }
        pending.extend(entries.into_iter().filter(|path| path.is_dir()));
    }
    None
}

fn find_dir_icon(root: &Path) -> Option<PathBuf> {
    let path = root.join(".DirIcon");
    let contents = fs::read(&path).ok()?;
    if is_supported_icon(&contents) {
        return Some(path);
    }

    // AppDir images may store .DirIcon as a text path instead of an image
    // symlink. Resolve it within the extracted image root, never outside it.
    let reference = std::str::from_utf8(&contents).ok()?.trim();
    if reference.is_empty() {
        return None;
    }
    let reference_path = Path::new(reference);
    let relative = reference_path.strip_prefix("/").unwrap_or(reference_path);
    let image_root = root.canonicalize().ok()?;
    let resolved = root.join(relative).canonicalize().ok()?;
    if !resolved.starts_with(image_root) {
        return None;
    }
    let contents = fs::read(&resolved).ok()?;
    is_supported_icon(&contents).then_some(resolved)
}

fn is_supported_icon(contents: &[u8]) -> bool {
    contents.starts_with(b"\x89PNG\r\n\x1a\n")
        || std::str::from_utf8(contents)
            .ok()
            .is_some_and(|text| text.contains("<svg"))
}

fn find_default_icon(root: &Path) -> Option<PathBuf> {
    find_dir_icon(root).or_else(|| find_named_icon(root, "icon"))
}

/// Copies an extracted icon beside the AppImage in Gear Lever's `.icons` folder.
pub fn copy_icon(source: &Path, appimage_path: &Path, desktop_file_path: &Path) -> Result<PathBuf> {
    let app_id = desktop_file_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| desktop::app_id_for("appimage", appimage_path));
    let appimages_directory = appimage_path
        .parent()
        .ok_or_else(|| anyhow!("AppImage path has no parent directory"))?;
    let directory = appimages_directory.join(".icons");
    fs::create_dir_all(&directory)?;
    let filename = source
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{app_id}.{extension}"))
        .unwrap_or(app_id);
    let destination = directory.join(filename);
    fs::copy(source, &destination)
        .with_context(|| format!("cannot copy icon to {}", destination.display()))?;
    Ok(destination)
}

/// Picks a destination file name inside `directory`, appending a numeric
/// suffix when the preferred name is already taken.
pub fn unique_destination(directory: &Path, name: &str) -> PathBuf {
    let first = preferred_destination(directory, name);
    if !first.exists() {
        return first;
    }
    let stem = first
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("appimage");
    for number in 2..10000 {
        let candidate = directory.join(format!("{stem}-{number}.appimage"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}.appimage", now_suffix()))
}

/// Returns the Gear Lever style base filename for an AppImage name.
pub fn preferred_destination(directory: &Path, name: &str) -> PathBuf {
    let name = name
        .strip_suffix(".AppImage")
        .or_else(|| name.strip_suffix(".appimage"))
        .unwrap_or(name);
    let safe = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let safe = if safe.is_empty() {
        "appimage".to_string()
    } else {
        safe
    };
    let filename = if safe.to_ascii_lowercase().ends_with(".appimage") {
        safe
    } else {
        format!("{safe}.appimage")
    };
    directory.join(filename)
}

/// Picks a versioned side-by-side filename when another release has the base
/// name already. If the same version is present, use a numeric suffix.
pub fn keep_both_destination(directory: &Path, name: &str, version: &str) -> PathBuf {
    let preferred = preferred_destination(directory, name);
    let stem = preferred
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("appimage");
    let version = version
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    let versioned = if version.is_empty() {
        format!("{stem}_copy.appimage")
    } else {
        format!("{stem}_{version}.appimage")
    };
    let first = directory.join(versioned);
    if !first.exists() {
        return first;
    }
    for number in 1..10000 {
        let candidate = directory.join(format!("{stem}_{number}.appimage"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}_{}.appimage", now_suffix()))
}

fn now_suffix() -> u128 {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    (timestamp << 16) | u128::from(UNIQUE_SUFFIX.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_appimage_type_and_architecture_without_external_tools() {
        let (directory, path) = fixture("Synthetic.AppImage");
        assert_eq!(detect_type(&path), AppImageType::Type2);
        assert_eq!(detect_architecture(&path), Architecture::X86_64);
        assert!(is_appimage(&path));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn inspects_synthetic_appimage_without_external_tools() {
        let (directory, path) = fixture("Synthetic.AppImage");
        let candidate = inspect(&path, false).unwrap();
        assert_eq!(candidate.name, "Synthetic");
        assert_eq!(candidate.hashes.md5.len(), 32);
        assert_eq!(candidate.hashes.sha1.len(), 40);
        assert_eq!(candidate.hashes.sha256.len(), 64);
        assert_eq!(
            inspect(&path, false).unwrap().hashes.sha256,
            candidate.hashes.sha256
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn appimage_rs_extracts_metadata_without_external_programs() {
        let directory = tempfile_directory();
        let app_dir = directory.join("Test.AppDir");
        fs::create_dir_all(&app_dir).unwrap();
        fs::write(
            app_dir.join("test.desktop"),
            "[Desktop Entry]\nType=Application\nName=Test App\nComment=Sample\nExec=test %U\nIcon=test-icon\n",
        )
        .unwrap();
        fs::write(
            app_dir.join("test-icon.svg"),
            "<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
        )
        .unwrap();

        let mut runtime = fs::read("/bin/true").unwrap();
        runtime[8..11].copy_from_slice(b"AI\x02");
        let image_path = directory.join("Test.AppImage");
        ::appimage::AppImage::pack(&app_dir)
            .runtime(runtime)
            .compression(::appimage::Compression::None)
            .write_to(&image_path)
            .unwrap();

        let candidate = inspect(&image_path, false).unwrap();
        assert_eq!(candidate.name, "Test App");
        assert_eq!(candidate.description.as_deref(), Some("Sample"));
        assert!(
            candidate
                .icon_path
                .as_ref()
                .is_some_and(|path| path.is_file())
        );
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn creates_unique_appimage_names() {
        let directory = tempfile_directory();
        let first = unique_destination(&directory, "Example App");
        fs::write(&first, b"placeholder").unwrap();
        let second = unique_destination(&directory, "Example App");
        assert_ne!(first, second);
        assert!(second.file_name().unwrap().to_string_lossy().contains("-2"));
        let _ = fs::remove_dir_all(directory);
    }

    fn tempfile_directory() -> PathBuf {
        let directory = std::env::temp_dir().join(format!("appimanager-test-{}", now_suffix()));
        fs::create_dir_all(&directory).unwrap();
        directory
    }

    fn fixture(name: &str) -> (PathBuf, PathBuf) {
        let directory = tempfile_directory();
        let path = directory.join(name);
        let mut bytes = vec![0u8; 128];
        bytes[0..4].copy_from_slice(b"\x7fELF");
        bytes[4] = 2;
        bytes[5] = 1;
        bytes[18..20].copy_from_slice(&62u16.to_le_bytes());
        bytes[8..11].copy_from_slice(b"AI\x02");
        fs::write(&path, bytes).unwrap();
        (directory, path)
    }
}
