//! Pure-Rust extraction of the embedded SquashFS via [`backhand`].
//!
//! Unlike the FUSE mount, this works on any platform: it walks the SquashFS
//! nodes and materialises directories, files, and symlinks on disk.

use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use backhand::{FilesystemReader, InnerNode, Node, NodeHeader, SquashfsFileReader};
use glob::Pattern;

use crate::compression;
use crate::error::Result;

pub(crate) fn extract(
    image: &Path,
    offset: u64,
    destination: &Path,
    pattern: Option<&str>,
    replacing: bool,
) -> Result<()> {
    let pattern = pattern.map(Pattern::new).transpose()?;
    let reader = BufReader::new(File::open(image)?);
    let filesystem = compression::open_reader(reader, offset)?;

    fs::create_dir_all(destination)?;
    for node in filesystem.files() {
        let Some(relative) = relative_path(&node.fullpath) else {
            continue; // the root itself
        };
        if let Some(pattern) = &pattern {
            if !pattern.matches(&relative.to_string_lossy()) {
                continue;
            }
        }
        let target = destination.join(&relative);
        write_node(&filesystem, node, &target, replacing)?;
    }
    Ok(())
}

fn write_node(
    filesystem: &FilesystemReader,
    node: &Node<SquashfsFileReader>,
    target: &Path,
    replacing: bool,
) -> Result<()> {
    match &node.inner {
        InnerNode::Dir(_) => {
            fs::create_dir_all(target)?;
            set_permissions(target, &node.header);
            restore_owner(target, &node.header);
            restore_mtime(target, &node.header);
        }
        InnerNode::File(file) => {
            ensure_parent(target)?;
            if !replacing && is_same_size(target, file.file_len()) {
                return Ok(());
            }
            let mut source = filesystem.file(file).reader();
            let mut sink = File::create(target)?;
            io::copy(&mut source, &mut sink)?;
            // Set mtime on the open handle before closing it.
            let _ = sink.set_modified(mtime_of(&node.header));
            drop(sink);
            set_permissions(target, &node.header);
            restore_owner(target, &node.header);
        }
        InnerNode::Symlink(symlink) => {
            ensure_parent(target)?;
            write_symlink(&symlink.link, target, replacing)?;
            restore_symlink_owner(target, &node.header);
        }
        // Device nodes, FIFOs, and sockets need privileges the extractor
        // cannot assume; a payload root never depends on them.
        _ => {}
    }
    Ok(())
}

/// Strip the leading `/` so SquashFS paths join under the destination.
fn relative_path(full: &Path) -> Option<PathBuf> {
    let relative = full.strip_prefix("/").unwrap_or(full);
    (!relative.as_os_str().is_empty()).then(|| relative.to_path_buf())
}

fn ensure_parent(target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn is_same_size(target: &Path, expected: usize) -> bool {
    fs::metadata(target).is_ok_and(|meta| meta.len() == expected as u64)
}

#[cfg(unix)]
fn set_permissions(target: &Path, header: &NodeHeader) {
    use std::os::unix::fs::PermissionsExt;
    let _ = fs::set_permissions(
        target,
        fs::Permissions::from_mode(u32::from(header.permissions)),
    );
}

#[cfg(not(unix))]
fn set_permissions(_target: &Path, _header: &NodeHeader) {}

fn mtime_of(header: &NodeHeader) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(u64::from(header.mtime))
}

/// Restore a directory's mtime by touching its handle (best effort).
fn restore_mtime(target: &Path, header: &NodeHeader) {
    if let Ok(handle) = File::open(target) {
        let _ = handle.set_modified(mtime_of(header));
    }
}

/// Restore ownership where permitted; ignore `EPERM` when not root.
#[cfg(unix)]
fn restore_owner(target: &Path, header: &NodeHeader) {
    let _ = std::os::unix::fs::chown(target, Some(header.uid), Some(header.gid));
}

#[cfg(unix)]
fn restore_symlink_owner(target: &Path, header: &NodeHeader) {
    let _ = std::os::unix::fs::lchown(target, Some(header.uid), Some(header.gid));
}

#[cfg(not(unix))]
fn restore_owner(_target: &Path, _header: &NodeHeader) {}

#[cfg(not(unix))]
fn restore_symlink_owner(_target: &Path, _header: &NodeHeader) {}

#[cfg(unix)]
fn write_symlink(link: &Path, target: &Path, replacing: bool) -> Result<()> {
    if replacing && fs::symlink_metadata(target).is_ok() {
        fs::remove_file(target)?;
    }
    std::os::unix::fs::symlink(link, target)?;
    Ok(())
}

#[cfg(not(unix))]
fn write_symlink(_link: &Path, _target: &Path, _replacing: bool) -> Result<()> {
    Ok(())
}
