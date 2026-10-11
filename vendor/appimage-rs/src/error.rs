use std::io;
use std::path::PathBuf;

use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

/// Failures from opening, unpacking, or mounting an AppImage.
#[derive(Debug, Error)]
pub enum Error {
    /// A filesystem operation failed.
    #[error(transparent)]
    Io(#[from] io::Error),
    /// The file is not a type-2 AppImage.
    #[error("{path}: {reason}")]
    InvalidImage {
        /// Path that failed to parse.
        path: PathBuf,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// The embedded SquashFS could not be read (bad image, or a compression
    /// backend that was not enabled — see the `xz` / `zstd` features).
    #[error("squashfs error: {0}")]
    Squashfs(#[from] backhand::BackhandError),
    /// The `matching` pattern was not a valid glob.
    #[error("invalid pattern: {0}")]
    InvalidPattern(#[from] glob::PatternError),
    /// SquashFS extraction failed to write the destination tree.
    #[error("failed to unpack {}", path.display())]
    UnpackFailed {
        /// Image that failed to unpack.
        path: PathBuf,
    },
    /// Building an AppImage failed (bad source tree, or no runtime supplied).
    #[error("failed to pack AppImage: {0}")]
    Pack(String),
    /// The FUSE filesystem could not be mounted.
    #[error("{0}")]
    MountFailed(String),
    /// FUSE mounting is only implemented on Linux.
    #[error("appimage mounting requires a Linux target")]
    UnsupportedPlatform,
}

impl Error {
    /// True when an operation is unavailable because the target is not Linux.
    #[must_use]
    pub fn is_unsupported_platform(&self) -> bool {
        matches!(self, Self::UnsupportedPlatform)
    }
}

pub(crate) fn unique_temp_dir(prefix: &str) -> Result<PathBuf> {
    let dir = tempfile::Builder::new()
        .prefix(&format!(".{prefix}-"))
        .tempdir()
        .map_err(Error::Io)?;
    // Persist the directory; callers own cleanup.
    Ok(dir.keep())
}
