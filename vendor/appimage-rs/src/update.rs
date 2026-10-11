//! Typed view of the `.upd_info` ELF section (AppImage update information).
//!
//! The section is a single `|`-separated line describing where to fetch
//! updates. This parses the well-known transports and preserves anything else
//! verbatim so callers can still inspect it.
//!
//! ```
//! use appimage::UpdateInformation;
//!
//! let gh = UpdateInformation::parse(
//!     "gh-releases-zsync|probono|AppImages|latest|Subsurface-*x86_64.AppImage.zsync",
//! )
//! .unwrap();
//! assert_eq!(gh.transport(), "gh-releases-zsync");
//! assert!(matches!(gh, UpdateInformation::GitHubReleases { .. }));
//!
//! assert_eq!(UpdateInformation::parse(""), None);
//! ```

/// Parsed AppImage update information (`.upd_info`).
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum UpdateInformation {
    /// A direct zsync URL: `zsync|<url>`.
    Zsync {
        /// The `.zsync` control-file URL.
        url: String,
    },
    /// GitHub releases: `gh-releases-zsync|<user>|<repo>|<release>|<filename>`.
    GitHubReleases {
        /// Repository owner.
        username: String,
        /// Repository name.
        repository: String,
        /// Release tag, or `latest`.
        release: String,
        /// zsync filename, may contain a `*` wildcard.
        filename: String,
    },
    /// Pling/OCS store: `pling-zsync|<product-id>|<filename>` (was `bintray-zsync`).
    Pling {
        /// OCS product id.
        product_id: String,
        /// zsync filename, may contain a `*` wildcard.
        filename: String,
    },
    /// An unrecognized transport, preserved field-by-field.
    Other {
        /// The leading transport token.
        transport: String,
        /// The remaining `|`-separated fields.
        fields: Vec<String>,
    },
}

impl UpdateInformation {
    /// Parse a `.upd_info` line. Returns `None` for empty input.
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        let raw = raw.trim().trim_end_matches('\0').trim();
        if raw.is_empty() {
            return None;
        }
        let fields: Vec<&str> = raw.split('|').collect();
        Some(match fields.as_slice() {
            ["zsync", url] => Self::Zsync {
                url: (*url).to_owned(),
            },
            ["gh-releases-zsync", username, repository, release, filename] => {
                Self::GitHubReleases {
                    username: (*username).to_owned(),
                    repository: (*repository).to_owned(),
                    release: (*release).to_owned(),
                    filename: (*filename).to_owned(),
                }
            }
            ["pling-zsync" | "bintray-zsync", product_id, filename] => Self::Pling {
                product_id: (*product_id).to_owned(),
                filename: (*filename).to_owned(),
            },
            [transport, rest @ ..] => Self::Other {
                transport: (*transport).to_owned(),
                fields: rest.iter().map(|field| (*field).to_owned()).collect(),
            },
            [] => return None,
        })
    }

    /// The transport token (`zsync`, `gh-releases-zsync`, …).
    #[must_use]
    pub fn transport(&self) -> &str {
        match self {
            Self::Zsync { .. } => "zsync",
            Self::GitHubReleases { .. } => "gh-releases-zsync",
            Self::Pling { .. } => "pling-zsync",
            Self::Other { transport, .. } => transport,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UpdateInformation::{self, GitHubReleases, Other, Pling, Zsync};

    #[test]
    fn parses_known_transports() {
        assert_eq!(
            UpdateInformation::parse("zsync|https://example.test/App.zsync"),
            Some(Zsync {
                url: "https://example.test/App.zsync".into()
            })
        );
        assert!(matches!(
            UpdateInformation::parse("gh-releases-zsync|u|r|latest|App-*.zsync"),
            Some(GitHubReleases { .. })
        ));
        assert!(matches!(
            UpdateInformation::parse("bintray-zsync|123|App-*.zsync"),
            Some(Pling { .. })
        ));
    }

    #[test]
    fn preserves_unknown_and_rejects_empty() {
        assert_eq!(UpdateInformation::parse("  \0"), None);
        match UpdateInformation::parse("future-transport|a|b").unwrap() {
            Other { transport, fields } => {
                assert_eq!(transport, "future-transport");
                assert_eq!(fields, ["a", "b"]);
            }
            other => panic!("expected Other, got {other:?}"),
        }
    }
}
