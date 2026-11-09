//! Level-2 error for [`disc`][crate::disc]: the [`DiscError`] enum & the [`TrackMismatch`] leaf.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error, data-carrying
//! variants rendered by `Display`, and `From<DiscError> for io::Error` preserving the call site's
//! previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind, RawOsError},
};

use syn::token::Not;
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
    /// The rip was cancelled via its [`Context`].
    Cancelled,
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
            DiscError::Cancelled => IoErrorKind::Interrupted,
            DiscError::Io(error) => error.kind(),
        }
    }
}

impl Display for DiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscError::TrackMismatch(mismatch) => Display::fmt(mismatch, f),
            DiscError::NoReleases => f.write_str("no releases found"),
            DiscError::Cancelled => f.write_str("thread cancellation requested by controller"),
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            DiscError::MusicBrainz(_) => f.write_str("MusicBrainz API error"),
            DiscError::Http(_) => f.write_str("HTTP error"),
            DiscError::Io(_) => f.write_str("I/O error"),
        }
    }
}

impl StdError for DiscError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DiscError::MusicBrainz(error) => Some(error),
            DiscError::Http(error) => Some(error),
            DiscError::Io(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            DiscError::TrackMismatch(_) | DiscError::NoReleases | DiscError::Cancelled => None,
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

impl<R> From<Context<R>> for DiscError {
    fn from(_: Context<R>) -> Self {
        DiscError::Cancelled
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

/// Errors returned by [`trait Device`][crate::Device]
#[derive(Debug)]
pub enum DeviceError {
    /// The provided/calculated buffer size is unsuitable or too large for this architecture
    InvalidBufferSize,
    /// The device is not ready - usually because no disc is inserted
    NotReady,
    /// Less data was read than expected
    UnexpectedEndOfRead {
        /// The number of bytes expected.
        expected: u32,
        /// The number of bytes actually received.
        received: u32,
    },
    /// The device does not support CD-ROM operations
    UnsupportedDevice,
    /// Some other io::Error occurred
    Other(io::Error),
}

impl StdError for DeviceError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        use DeviceError::*;

        match self {
            Other(e) => Some(e),
            InvalidBufferSize | NotReady | UnexpectedEndOfRead { .. } | UnsupportedDevice => None,
        }
    }
}

impl Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::InvalidBufferSize => todo!(),
            DeviceError::NotReady => write!(f, "device not ready, is a disc inserted?"),
            DeviceError::UnexpectedEndOfRead { expected, received } => {
                write!(f, "expected {expected} bytes, received {received}")
            }
            DeviceError::UnsupportedDevice => write!(f, "device does not support CD operations"),
            DeviceError::Other(error) => Display::fmt(error, f),
        }
    }
}

/// Trait for Errors which provide OS-colouring around an io::Error
///
/// # Note for implementors
///
/// Implementations must guarantee that construction is only possible from a genuine OS error - either
/// via [`io::Error::last_os_error`] or [`io::Error::from_raw_os_error`], otherwise the provided
/// function [`error_code`][OsError::error_code] will panic.
///
/// Most implementations will be a simple new-type wrapper to provide colouring
/// TODO: include WinError as full example
pub trait OsError:
    StdError + Send + Sync + 'static // C-GOOD-ERR
    + From<io::Error>
    + Into<DeviceError>
{
    /// Obtain a reference to the underlying io::Error
    fn as_io_error(&self) -> &io::Error;

    /// Consume the OsError wrapper and return the wrapped io::Error
    fn into_io_error(self) -> io::Error;

    /// Generate a new, owned, io::Error based on the underlying error code
    /// 
    /// This is a workaround the fact that io::Error is not clone
    fn to_io_error(&self) -> io::Error {
        io::Error::from_raw_os_error(self.error_code())
    }

    /// Obtain the raw os error code related to this error.
    fn error_code(&self) -> RawOsError {
        self.as_io_error().raw_os_error().expect("Construction only possible from a raw os error")
    }
}

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
