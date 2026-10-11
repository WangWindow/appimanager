//! An opened type-2 AppImage and the jobs that consume it.
//!
//! Opening, reading ELF sections, and unpacking are pure Rust and work on any
//! platform. Mounting uses FUSE and is implemented on Linux; elsewhere
//! [`AppImage::mount`] returns [`Error::UnsupportedPlatform`].
//!
//! ```
//! use appimage::{AppImage, Error};
//!
//! // `/dev/null` is not an ELF, so opening fails the same way everywhere.
//! match AppImage::open("/dev/null") {
//!     Err(Error::InvalidImage { .. } | Error::Io(_)) => {}
//!     other => panic!("unexpected: {other:?}"),
//! }
//! ```

use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::payload::{Config, Home, Payload, Prepared, Sidecar, SidecarDir, UnpackTarget};
use crate::types::Section;
use crate::update::UpdateInformation;
use crate::{elf, extract};

/// An opened type-2 AppImage: a path and the offset of its SquashFS payload.
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AppImage {
    path: PathBuf,
    offset: u64,
}

impl AppImage {
    /// Open a type-2 AppImage and locate its SquashFS payload.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let offset = elf::offset(&path)?;
        Ok(Self { path, offset })
    }

    /// Begin building an AppImage from an AppDir. Completes at
    /// [`Packer::write_to`].
    pub fn pack(dir: impl AsRef<Path>) -> crate::pack::Packer {
        crate::pack::Packer::new(dir.as_ref().to_path_buf())
    }

    /// Byte offset of the embedded SquashFS image.
    #[must_use]
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Path originally passed to [`AppImage::open`].
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Read an ELF section. Missing or empty sections are `Ok(None)`.
    pub fn section(&self, name: &str) -> Result<Option<Section>> {
        Ok(elf::section(&self.path, name)?.map(Section::from_bytes))
    }

    /// Raw `.upd_info` payload, when present.
    pub fn update_information(&self) -> Result<Option<Section>> {
        self.section(".upd_info")
    }

    /// Parsed `.upd_info` update information, when present.
    pub fn update_info(&self) -> Result<Option<UpdateInformation>> {
        Ok(self
            .update_information()?
            .and_then(|section| UpdateInformation::parse(&section.to_string_lossy())))
    }

    /// `.sha256_sig` signature payload, when present.
    pub fn signature(&self) -> Result<Option<Section>> {
        self.section(".sha256_sig")
    }

    /// `.sig_key` public key used to verify [`Self::signature`], when present.
    pub fn signing_key(&self) -> Result<Option<Section>> {
        self.section(".sig_key")
    }

    /// Begin an unpack. Completes only at [`Unpack::to`].
    pub fn unpack(&self) -> Unpack<'_> {
        Unpack {
            image: self,
            pattern: None,
            replacing: false,
        }
    }

    /// Mount on a unique temporary directory. Dropping the mount unmounts it.
    pub fn mount(&self) -> Result<Mount> {
        self.mount_at_inner(None)
    }

    /// Mount on an existing empty directory.
    pub fn mount_at(&self, mountpoint: impl AsRef<Path>) -> Result<Mount> {
        self.mount_at_inner(Some(mountpoint.as_ref().to_path_buf()))
    }

    #[cfg(not(target_os = "linux"))]
    fn mount_at_inner(&self, mountpoint: Option<PathBuf>) -> Result<Mount> {
        let _ = mountpoint;
        Err(Error::UnsupportedPlatform)
    }

    #[cfg(target_os = "linux")]
    fn mount_at_inner(&self, mountpoint: Option<PathBuf>) -> Result<Mount> {
        let (mountpoint, owned) = match mountpoint {
            Some(path) => (path, false),
            None => (crate::error::unique_temp_dir("appimage-mount")?, true),
        };
        match crate::mount::mount(&self.path, self.offset, &mountpoint) {
            Ok(session) => Ok(Mount {
                path: mountpoint,
                owned,
                session: Some(session),
            }),
            Err(error) => {
                if owned {
                    let _ = std::fs::remove_dir(&mountpoint);
                }
                Err(error)
            }
        }
    }

    /// Adjacent `<AppImage>.home` directory.
    #[must_use]
    pub fn home(&self) -> Adjacent<'_, Home> {
        Adjacent {
            image: self,
            _kind: PhantomData,
        }
    }

    /// Adjacent `<AppImage>.config` directory.
    #[must_use]
    pub fn config(&self) -> Adjacent<'_, Config> {
        Adjacent {
            image: self,
            _kind: PhantomData,
        }
    }
}

impl TryFrom<&Path> for AppImage {
    type Error = Error;

    fn try_from(path: &Path) -> Result<Self> {
        Self::open(path)
    }
}

impl TryFrom<PathBuf> for AppImage {
    type Error = Error;

    fn try_from(path: PathBuf) -> Result<Self> {
        Self::open(path)
    }
}

impl AsRef<Path> for AppImage {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

/// In-progress unpack. The destination is required by [`Unpack::to`].
#[must_use = "call Unpack::to to write the payload"]
#[derive(Clone, Debug)]
pub struct Unpack<'a> {
    image: &'a AppImage,
    pattern: Option<String>,
    replacing: bool,
}

impl Unpack<'_> {
    /// Only emit SquashFS paths matching this `fnmatch` pattern.
    pub fn matching(mut self, pattern: impl Into<String>) -> Self {
        self.pattern = Some(pattern.into());
        self
    }

    /// Replace existing files instead of skipping equal-sized ones.
    pub fn replacing(mut self) -> Self {
        self.replacing = true;
        self
    }

    /// Unpack into `target`: a filesystem path ([`crate::AppDir`]) or
    /// [`crate::Temp`] ([`crate::Scratch`]).
    pub fn to<T: UnpackTarget>(self, target: T) -> Result<T::Output> {
        let Prepared {
            path,
            scratch,
            finish,
        } = target.prepare()?;
        match extract::extract(
            &self.image.path,
            self.image.offset,
            &path,
            self.pattern.as_deref(),
            self.replacing,
        ) {
            Ok(()) => Ok(finish(path)),
            Err(error) => {
                if scratch {
                    let _ = std::fs::remove_dir_all(&path);
                }
                Err(error)
            }
        }
    }
}

/// Typed sidecar next to the AppImage file.
#[derive(Clone, Copy, Debug)]
pub struct Adjacent<'a, Kind> {
    image: &'a AppImage,
    _kind: PhantomData<Kind>,
}

impl<Kind: Sidecar> Adjacent<'_, Kind> {
    /// Path of the sidecar, whether or not it exists.
    #[must_use]
    pub fn path(&self) -> PathBuf {
        let mut directory = self.image.path.as_os_str().to_os_string();
        directory.push(format!(".{}", Kind::SUFFIX));
        PathBuf::from(directory)
    }

    /// Whether the sidecar directory already exists.
    #[must_use]
    pub fn exists(&self) -> bool {
        self.path().is_dir()
    }

    /// Create the directory if needed and return a typed handle.
    pub fn ensure(&self) -> Result<SidecarDir<Kind>> {
        let directory = self.path();
        match std::fs::create_dir(&directory) {
            Ok(()) => Ok(SidecarDir::from_path(directory)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(SidecarDir::from_path(directory))
            }
            Err(error) => Err(error.into()),
        }
    }

    /// Open the sidecar if it already exists.
    pub fn open(&self) -> Result<Option<SidecarDir<Kind>>> {
        let directory = self.path();
        if directory.is_dir() {
            Ok(Some(SidecarDir::from_path(directory)))
        } else {
            Ok(None)
        }
    }
}

/// A live FUSE mount of the SquashFS payload. Dropping it unmounts.
pub struct Mount {
    path: PathBuf,
    owned: bool,
    #[cfg(target_os = "linux")]
    session: Option<crate::mount::BackgroundSession>,
}

impl Mount {
    /// Local mountpoint.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Keep the mountpoint directory after unmount (only for auto-created temps).
    #[must_use]
    pub fn into_path(mut self) -> PathBuf {
        self.owned = false;
        self.path.clone()
        // Drop still unmounts the FUSE session.
    }
}

impl Payload for Mount {
    fn path(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for Mount {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl std::ops::Deref for Mount {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl fmt::Debug for Mount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Mount")
            .field("path", &self.path)
            .field("owned", &self.owned)
            .finish()
    }
}

impl Drop for Mount {
    fn drop(&mut self) {
        // Unmount first (dropping the session), then remove an owned temp dir.
        #[cfg(target_os = "linux")]
        drop(self.session.take());
        if self.owned {
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
