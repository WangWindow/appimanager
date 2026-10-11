//! Bytes copied out of an ELF section.
//!
//! ```
//! use appimage::Section;
//!
//! let section = Section::from_bytes(b"update-info\0\0");
//! assert!(!section.is_empty());
//! assert_eq!(section.to_string_lossy(), "update-info");
//! assert_eq!(section.as_bytes().len(), 13);
//! ```

use std::fmt;
use std::ops::Deref;

/// Bytes copied out of an ELF section.
#[derive(Clone, Debug, Default, Eq, PartialEq, Hash)]
pub struct Section(pub(crate) Vec<u8>);

impl Section {
    /// Build a section from raw bytes (useful in tests).
    #[must_use]
    pub fn from_bytes(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Borrow the section bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Interpret the section as UTF-8 text when possible.
    pub fn to_str(&self) -> Result<&str, std::str::Utf8Error> {
        std::str::from_utf8(&self.0)
    }

    /// Lossy UTF-8 view with trailing NULs trimmed (common for `.upd_info`).
    #[must_use]
    pub fn to_string_lossy(&self) -> std::borrow::Cow<'_, str> {
        let trimmed = self.0.as_slice();
        let end = trimmed
            .iter()
            .rposition(|&byte| byte != 0)
            .map_or(0, |index| index + 1);
        String::from_utf8_lossy(&trimmed[..end])
    }

    /// Whether the section is empty or only NULs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|&byte| byte == 0)
    }

    /// Consume the section and return the raw bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

impl AsRef<[u8]> for Section {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl Deref for Section {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.as_bytes()
    }
}

impl From<Section> for Vec<u8> {
    fn from(section: Section) -> Self {
        section.0
    }
}

impl fmt::Display for Section {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_str() {
            Ok(text) => formatter.write_str(text.trim_end_matches('\0')),
            Err(_) => write!(formatter, "{:?}", self.0),
        }
    }
}
