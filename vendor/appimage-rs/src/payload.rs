//! Payload roots: an unpacked AppDir, a scratch directory, or a FUSE mount.
//!
//! [`Unpack::to`] is generic over [`UnpackTarget`] so a filesystem path and
//! [`Temp`] produce different owned types (`AppDir` vs [`Scratch`]) at compile
//! time. All roots are [`AsRef<Path>`] and share [`Payload`].
//!
//! ```
//! use appimage::{Config, Home, Sidecar, SidecarDir};
//! use std::path::PathBuf;
//!
//! assert_eq!(<Home as Sidecar>::SUFFIX, "home");
//! assert_eq!(<Config as Sidecar>::SUFFIX, "config");
//!
//! let home = SidecarDir::<Home>::from_path("/tmp/demo.AppImage.home");
//! assert_eq!(home.path(), PathBuf::from("/tmp/demo.AppImage.home"));
//! ```

use std::marker::PhantomData;
use std::ops::Deref;
use std::path::{Path, PathBuf};

use crate::error::{unique_temp_dir, Result};

/// Filesystem tree produced from an AppImage payload.
pub trait Payload: AsRef<Path> {
    /// Root of the payload (`AppDir`).
    fn path(&self) -> &Path;

    /// Join a path under the payload root.
    fn join(&self, rest: impl AsRef<Path>) -> PathBuf {
        self.path().join(rest)
    }

    /// Conventional `AppRun` entry inside the payload.
    fn app_run(&self) -> PathBuf {
        self.join("AppRun")
    }

    /// Whether `AppRun` exists under this root.
    fn has_app_run(&self) -> bool {
        self.app_run().is_file()
    }
}

macro_rules! path_root {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            path: PathBuf,
        }

        impl $name {
            pub(crate) fn from_path(path: PathBuf) -> Self {
                Self { path }
            }

            /// Consume the handle and return the directory path.
            #[must_use]
            pub fn into_path(self) -> PathBuf {
                self.path
            }
        }

        impl Payload for $name {
            fn path(&self) -> &Path {
                &self.path
            }
        }

        impl AsRef<Path> for $name {
            fn as_ref(&self) -> &Path {
                self.path()
            }
        }

        impl Deref for $name {
            type Target = Path;

            fn deref(&self) -> &Self::Target {
                self.path()
            }
        }

        impl From<$name> for PathBuf {
            fn from(value: $name) -> Self {
                value.path
            }
        }
    };
}

path_root! {
    /// Unpacked payload that the caller owns on disk.
    AppDir
}

/// Marker for [`Unpack::to`]: unpack into a unique temporary directory.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Temp;

/// Temporary unpacked payload. Removing the last handle deletes the directory.
#[derive(Debug)]
pub struct Scratch {
    inner: AppDir,
}

impl Scratch {
    pub(crate) fn from_path(path: PathBuf) -> Self {
        Self {
            inner: AppDir::from_path(path),
        }
    }

    /// Keep the directory and return a persistent [`AppDir`].
    #[must_use]
    pub fn persist(self) -> AppDir {
        let path = self.inner.path.clone();
        std::mem::forget(self);
        AppDir::from_path(path)
    }

    /// Consume and return the path without deleting (alias of [`Self::persist`] + [`AppDir::into_path`]).
    #[must_use]
    pub fn into_path(self) -> PathBuf {
        self.persist().into_path()
    }
}

impl Payload for Scratch {
    fn path(&self) -> &Path {
        self.inner.path()
    }
}

impl AsRef<Path> for Scratch {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Deref for Scratch {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.inner.path);
    }
}

/// Destination for [`crate::Unpack::to`].
///
/// Paths become [`AppDir`]. [`Temp`] becomes [`Scratch`].
pub trait UnpackTarget {
    /// Handle returned after a successful unpack.
    type Output: Payload;

    #[doc(hidden)]
    fn prepare(self) -> Result<Prepared<Self::Output>>;
}

#[doc(hidden)]
pub struct Prepared<O> {
    pub(crate) path: PathBuf,
    /// Whether `path` is a temp we own and should clean up on failure.
    pub(crate) scratch: bool,
    pub(crate) finish: fn(PathBuf) -> O,
}

impl<P: AsRef<Path>> UnpackTarget for P {
    type Output = AppDir;

    fn prepare(self) -> Result<Prepared<AppDir>> {
        Ok(Prepared {
            path: self.as_ref().to_path_buf(),
            scratch: false,
            finish: AppDir::from_path,
        })
    }
}

impl UnpackTarget for Temp {
    type Output = Scratch;

    fn prepare(self) -> Result<Prepared<Scratch>> {
        Ok(Prepared {
            path: unique_temp_dir("appimage-unpack")?,
            scratch: true,
            finish: Scratch::from_path,
        })
    }
}

/// Adjacent `<AppImage>.home` directory marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Home;

/// Adjacent `<AppImage>.config` directory marker.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config;

/// Owned adjacent directory keyed by [`Sidecar`] kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SidecarDir<K> {
    path: PathBuf,
    _kind: PhantomData<K>,
}

/// Owned adjacent `.home` directory.
pub type HomeDir = SidecarDir<Home>;

/// Owned adjacent `.config` directory.
pub type ConfigDir = SidecarDir<Config>;

impl<K> SidecarDir<K> {
    /// Wrap an existing adjacent directory path.
    #[must_use]
    pub fn from_path(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _kind: PhantomData,
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn into_path(self) -> PathBuf {
        self.path
    }
}

impl<K> AsRef<Path> for SidecarDir<K> {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl<K> Deref for SidecarDir<K> {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.path
    }
}

/// Kind of directory stored next to an AppImage (`foo.AppImage.home`).
pub trait Sidecar {
    /// Suffix after the AppImage path, without the leading dot.
    const SUFFIX: &'static str;
}

impl Sidecar for Home {
    const SUFFIX: &'static str = "home";
}

impl Sidecar for Config {
    const SUFFIX: &'static str = "config";
}
