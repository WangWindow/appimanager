#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc = include_str!("../README.md")]
#![deny(unsafe_op_in_unsafe_fn)]

mod compression;
mod elf;
mod error;
mod extract;
mod image;
mod pack;
mod payload;
mod types;
mod update;

#[cfg(target_os = "linux")]
mod mount;

pub use error::{Error, Result};
pub use image::{Adjacent, AppImage, Mount, Unpack};
pub use pack::{Compression, Packer};
pub use payload::{
    AppDir, Config, ConfigDir, Home, HomeDir, Payload, Scratch, Sidecar, SidecarDir, Temp,
    UnpackTarget,
};
pub use types::Section;
pub use update::UpdateInformation;
