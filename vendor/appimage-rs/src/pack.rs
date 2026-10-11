//! Build a type-2 AppImage: a runtime ELF with a SquashFS of an AppDir
//! appended. Mirrors what `appimagetool` does, in pure Rust via `backhand`'s
//! writer. All offered compressors are pure Rust; the runtime is supplied by
//! the caller (the official AppImage runtime, or any ELF whose size marks the
//! payload boundary).
//!
//! ```no_run
//! use appimage::{AppImage, Compression};
//!
//! let image = AppImage::pack("MyApp.AppDir")
//!     .runtime(std::fs::read("runtime-x86_64")?)
//!     .compression(Compression::Gzip)
//!     .write_to("MyApp.AppImage")?;
//! assert!(image.offset() > 0);
//! # Ok::<(), appimage::Error>(())
//! ```

use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use backhand::compression::{CompressionOptions, Compressor, Lz4};
use backhand::{FilesystemCompressor, FilesystemWriter, NodeHeader};

use crate::error::{Error, Result};
use crate::AppImage;

/// SquashFS compression for packing. All variants are pure Rust.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[non_exhaustive]
pub enum Compression {
    /// DEFLATE via `zlib-rs` — the widely-compatible default.
    #[default]
    Gzip,
    /// LZ4 via `lz4_flex` — fastest, larger.
    Lz4,
    /// LZO via `rust-lzo`.
    Lzo,
    /// No compression.
    None,
}

impl Compression {
    fn compressor(self) -> Compressor {
        match self {
            Self::Gzip => Compressor::Gzip,
            Self::Lz4 => Compressor::Lz4,
            Self::Lzo => Compressor::Lzo,
            Self::None => Compressor::Uncompressed,
        }
    }

    /// Compression options to pair with [`Self::compressor`]. Only `Lz4`
    /// requires one — backhand's `FilesystemCompressor::new` rejects
    /// `(Compressor::Lz4, None)` outright — and the field values themselves
    /// are otherwise unused by backhand's `lz4_flex`-backed (de)compressor.
    fn options(self) -> Option<CompressionOptions> {
        match self {
            Self::Lz4 => Some(CompressionOptions::Lz4(Lz4 {
                version: 1,
                flags: 0,
            })),
            Self::Gzip | Self::Lzo | Self::None => None,
        }
    }
}

/// Builder for [`AppImage::pack`]. Finish with [`Packer::write_to`].
#[must_use = "call Packer::write_to to produce the AppImage"]
pub struct Packer {
    dir: PathBuf,
    runtime: Option<Vec<u8>>,
    compression: Compression,
    block_size: Option<u32>,
}

impl Packer {
    pub(crate) fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            runtime: None,
            compression: Compression::Gzip,
            block_size: None,
        }
    }

    /// Prepend the runtime ELF (the official AppImage runtime, or any ELF).
    /// Read it yourself first: `.runtime(std::fs::read(path)?)`.
    pub fn runtime(mut self, bytes: impl Into<Vec<u8>>) -> Self {
        self.runtime = Some(bytes.into());
        self
    }

    /// Compression for the SquashFS payload (default [`Compression::Gzip`]).
    pub fn compression(mut self, compression: Compression) -> Self {
        self.compression = compression;
        self
    }

    /// SquashFS data block size (default backhand chooses 128 KiB).
    pub fn block_size(mut self, bytes: u32) -> Self {
        self.block_size = Some(bytes);
        self
    }

    /// Write the AppImage to `output` and return a handle to it.
    pub fn write_to(self, output: impl AsRef<Path>) -> Result<AppImage> {
        let output = output.as_ref();
        let runtime = self
            .runtime
            .ok_or_else(|| Error::Pack("a runtime is required; call .runtime()".to_owned()))?;
        let squashfs = build_squashfs(&self.dir, self.compression, self.block_size)?;

        let mut file = File::create(output)?;
        file.write_all(&runtime)?;
        file.write_all(&squashfs)?;
        file.flush()?;
        drop(file);
        make_executable(output);

        AppImage::open(output)
    }
}

fn build_squashfs(
    dir: &Path,
    compression: Compression,
    block_size: Option<u32>,
) -> Result<Vec<u8>> {
    if !dir.is_dir() {
        return Err(Error::Pack(format!("{} is not a directory", dir.display())));
    }
    let mut writer = FilesystemWriter::default();
    writer.set_compressor(
        FilesystemCompressor::new(compression.compressor(), compression.options())
            .map_err(pack_err)?,
    );
    if let Some(block_size) = block_size {
        writer.set_block_size(block_size);
    }
    add_tree(&mut writer, dir, dir)?;

    let mut out = Cursor::new(Vec::new());
    writer.write(&mut out).map_err(pack_err)?;
    Ok(out.into_inner())
}

/// Recursively add `current`'s contents to the writer, relative to `root`.
fn add_tree(writer: &mut FilesystemWriter, root: &Path, current: &Path) -> Result<()> {
    let mut entries: Vec<_> = fs::read_dir(current)?.collect::<std::io::Result<_>>()?;
    entries.sort_by_key(std::fs::DirEntry::file_name); // deterministic output

    for entry in entries {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);
        let metadata = fs::symlink_metadata(&path)?;
        let file_type = metadata.file_type();
        let header = header_of(&metadata, &file_type);

        if file_type.is_symlink() {
            let target = fs::read_link(&path)?;
            writer
                .push_symlink(target, relative, header)
                .map_err(pack_err)?;
        } else if file_type.is_dir() {
            writer.push_dir(relative, header).map_err(pack_err)?;
            add_tree(writer, root, &path)?;
        } else if file_type.is_file() {
            let source = File::open(&path)?;
            writer
                .push_file(source, relative, header)
                .map_err(pack_err)?;
        }
        // Skip device nodes / FIFOs / sockets: an AppDir never contains them.
    }
    Ok(())
}

#[cfg(unix)]
fn header_of(metadata: &fs::Metadata, _file_type: &fs::FileType) -> NodeHeader {
    use std::os::unix::fs::MetadataExt;
    NodeHeader::new(
        (metadata.mode() & 0o7777) as u16,
        metadata.uid(),
        metadata.gid(),
        metadata.mtime() as u32,
    )
}

#[cfg(not(unix))]
fn header_of(metadata: &fs::Metadata, file_type: &fs::FileType) -> NodeHeader {
    use std::time::UNIX_EPOCH;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |delta| delta.as_secs() as u32);
    let mode = if file_type.is_dir() { 0o755 } else { 0o644 };
    NodeHeader::new(mode, 0, 0, mtime)
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(permissions.mode() | 0o755);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn pack_err(error: backhand::BackhandError) -> Error {
    Error::Pack(error.to_string())
}
