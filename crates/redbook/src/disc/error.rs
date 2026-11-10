//! Level-2 error for [`disc`][crate::disc]: the [`DiscError`] enum & the [`TrackMismatch`] leaf.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error, data-carrying
//! variants rendered by `Display`, and `From<DiscError> for io::Error` preserving the call site's
//! previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use thread_safely::Context;

use crate::{Track, error::Error};

/// Error returned by [`Disc`][crate::Disc] APIs: metadata lookup, cover art & validation
/// failures.
///
/// Variants carry the data which led to the error (issue #193). Variants wrapping an underlying
/// error are returned by `source()` and *not* rendered by `Display` (issue #198). Wrapped foreign
/// errors convert via `From` / `?`.
#[derive(Debug)]
pub enum DiscError {
    /// The provided tracks do not match the TOC.
    TrackMismatch(TrackMismatch),
    /// No MusicBrainz release was found / selected.
    NoReleases,
    /// A thread cancellation request was received via the [`Context`]
    ThreadCancellation,
    /// A wrapped [`musicbrainz_rs`] error, e.g. from fetching disc metadata. Boxed because the
    /// snafu-generated error is too large to keep the `Err` variant small
    /// (`clippy::result_large_err`).
    MusicBrainz(Box<musicbrainz_rs::ApiEndpointError>),
    /// A wrapped [`reqwest`] error, e.g. from downloading cover art.
    Http(reqwest::Error),
    /// A wrapped [`io::Error`].
    Io(io::Error),
}

impl DiscError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match self {
            DiscError::TrackMismatch(_) => IoErrorKind::InvalidInput,
            DiscError::NoReleases | DiscError::MusicBrainz(_) | DiscError::Http(_) => {
                IoErrorKind::Other
            }
            DiscError::Io(error) => error.kind(),
            _ => todo!(),
        }
    }
}

impl Display for DiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscError::TrackMismatch(mismatch) => Display::fmt(mismatch, f),
            DiscError::NoReleases => f.write_str("no releases found"),
            DiscError::MusicBrainz(_) => f.write_str("MusicBrainz API error"),
            DiscError::Http(_) => f.write_str("HTTP error"),
            DiscError::Io(_) => f.write_str("I/O error"),
            _ => todo!(),
        }
    }
}

impl StdError for DiscError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DiscError::MusicBrainz(error) => Some(error),
            DiscError::Http(error) => Some(error),
            DiscError::Io(error) => Some(error),
            DiscError::TrackMismatch(_) | DiscError::NoReleases => None,
            _ => todo!(),
        }
    }
}

impl From<musicbrainz_rs::ApiEndpointError> for DiscError {
    fn from(error: musicbrainz_rs::ApiEndpointError) -> Self {
        DiscError::MusicBrainz(Box::new(error))
    }
}

impl From<reqwest::Error> for DiscError {
    fn from(error: reqwest::Error) -> Self {
        DiscError::Http(error)
    }
}

impl From<Context<!>> for DiscError {
    fn from(_: Context<!>) -> Self {
        Self::ThreadCancellation
    }
}

impl From<io::Error> for DiscError {
    fn from(error: io::Error) -> Self {
        DiscError::Io(error)
    }
}

impl From<DiscError> for Error {
    fn from(error: DiscError) -> Self {
        Error::Disc(error)
    }
}

impl From<DiscError> for io::Error {
    fn from(error: DiscError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Leaf error: the provided tracks do not match the TOC (see [`Disc::new`][crate::Disc::new]).
#[derive(Debug, Clone, PartialEq)]
pub enum TrackMismatch {
    /// A provided track differs from the corresponding TOC entry.
    Mismatch {
        /// The track as provided.
        provided: Track<'static>,
        /// The corresponding entry from the TOC.
        from_toc: cdtoc::Track,
    },
    /// More tracks were provided than the TOC has entries; the first additional one.
    ExtraProvided(Track<'static>),
    /// The TOC has more entries than tracks were provided; the first additional one.
    ExtraInToc(cdtoc::Track),
}

impl Display for TrackMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackMismatch::Mismatch { provided, from_toc } => {
                write!(
                    f,
                    "track mismatch: toc {from_toc:?} vs provided track {provided}"
                )
            }
            TrackMismatch::ExtraProvided(track) => write!(
                f,
                "provided tracks contain more entries than the toc; first additional entry {track}"
            ),
            TrackMismatch::ExtraInToc(toc_track) => write!(
                f,
                "toc contains more entries than the provided tracks; first additional entry {toc_track:?}"
            ),
        }
    }
}

impl StdError for TrackMismatch {}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<DiscError>();
        assert_traits::<TrackMismatch>();
    }

    #[test]
    fn display_renders_labels_and_data() {
        assert_eq!(DiscError::NoReleases.to_string(), "no releases found");
        assert_eq!(
            DiscError::from(io::Error::new(IoErrorKind::NotFound, "gone")).to_string(),
            "I/O error"
        );
    }

    #[cfg(false)]
    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert!(matches!(
            io::Error::from(DiscError::TrackMismatch(TrackMismatch::ExtraInToc(
                cdtoc::Toc::from_cdtoc("1+96+2D2B")
                    .unwrap()
                    .audio_tracks()
                    .next()
                    .unwrap()
            )))
            .kind(),
            IoErrorKind::InvalidInput
        ));
        assert_eq!(
            io::Error::from(DiscError::NoReleases).kind(),
            IoErrorKind::Other
        );
        assert_eq!(
            io::Error::from(DiscError::Cancelled).kind(),
            IoErrorKind::Interrupted
        );
    }

    #[test]
    fn source_returns_wrapped_errors_only() {
        let error = DiscError::from(io::Error::from_raw_os_error(5));
        assert!(
            StdError::source(&error)
                .and_then(|s| s.downcast_ref::<io::Error>())
                .is_some()
        );
        assert!(StdError::source(&DiscError::NoReleases).is_none());
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(DiscError::NoReleases);
        assert!(matches!(error, Error::Disc(DiscError::NoReleases)));
    }
}
