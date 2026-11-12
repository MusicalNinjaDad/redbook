#![expect(missing_docs, reason = "TODO")]
//! Level-2 error for [`disc`][crate::disc]: the [`DiscError`] enum & the [`TocMismatch`] leaf.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error and
//! data-carrying variants rendered by `Display`. Conversion to [`io::Error`] is explicit via
//! [`RedbookError::into_io_error`][crate::error::RedbookError::into_io_error], never via
//! `From<_> for io::Error`.
use std::{
    error::Error,
    fmt::{self, Display},
    io,
};

use thread_safely::Context;

use crate::{Track, error::RedbookError};

/// Error returned by [`Disc`][crate::Disc] APIs: metadata lookup, cover art & validation
/// failures.
#[derive(Debug)]
pub enum DiscError {
    /// The provided tracks do not match the TOC.
    TocMismatch(TocMismatch),
    TaggingError(TaggingError),
    /// No MusicBrainz release was found / selected.
    NoReleases,
    /// A thread cancellation request was received via the [`Context`]
    ThreadCancellation,
    IoError(IoError),
}

impl RedbookError for DiscError {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        match self {
            DiscError::TocMismatch(_) => io::ErrorKind::InvalidInput,
            DiscError::TaggingError(_) => io::ErrorKind::InvalidData,
            DiscError::NoReleases => io::ErrorKind::Other,
            DiscError::ThreadCancellation => io::ErrorKind::Interrupted,
            DiscError::IoError(_) => io::ErrorKind::Other,
        }
    }
}

impl Display for DiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscError::TocMismatch(_) => write!(f, "provided tracks do not match the TOC"),
            DiscError::TaggingError(_) => write!(f, "tagging error"),
            DiscError::IoError(_) => write!(f, "I/O error"),
            DiscError::NoReleases => write!(f, "no releases found"),
            DiscError::ThreadCancellation => write!(f, "thread cancelled"),
        }
    }
}

impl Error for DiscError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DiscError::TocMismatch(error) => Some(error),
            DiscError::TaggingError(error) => Some(error),
            DiscError::IoError(error) => Some(error),
            DiscError::NoReleases | DiscError::ThreadCancellation => None,
        }
    }
}

impl From<TocMismatch> for DiscError {
    fn from(error: TocMismatch) -> Self {
        Self::TocMismatch(error)
    }
}

impl From<TaggingError> for DiscError {
    fn from(error: TaggingError) -> Self {
        Self::TaggingError(error)
    }
}

impl From<UnknownMimeType> for DiscError {
    fn from(error: UnknownMimeType) -> Self {
        TaggingError::from(error).into()
    }
}

impl From<metaflac::Error> for DiscError {
    fn from(error: metaflac::Error) -> Self {
        TaggingError::from(error).into()
    }
}

impl From<Context<!>> for DiscError {
    fn from(_: Context<!>) -> Self {
        Self::ThreadCancellation
    }
}

impl From<IoError> for DiscError {
    fn from(error: IoError) -> Self {
        Self::IoError(error)
    }
}

impl From<musicbrainz_rs::ApiEndpointError> for DiscError {
    fn from(error: musicbrainz_rs::ApiEndpointError) -> Self {
        IoError::from(error).into()
    }
}

impl From<reqwest::Error> for DiscError {
    fn from(error: reqwest::Error) -> Self {
        IoError::from(error).into()
    }
}

impl From<io::Error> for DiscError {
    fn from(error: io::Error) -> Self {
        IoError::from(error).into()
    }
}

#[derive(Debug)]
pub enum TaggingError {
    UnknownMimeType(UnknownMimeType),
    MetaflacError(metaflac::Error),
}

impl Error for TaggingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TaggingError::UnknownMimeType(error) => Some(error),
            TaggingError::MetaflacError(error) => Some(error),
        }
    }
}

impl Display for TaggingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaggingError::UnknownMimeType(_) => write!(f, "unknown MIME type"),
            TaggingError::MetaflacError(_) => write!(f, "metaflac error"),
        }
    }
}

impl From<UnknownMimeType> for TaggingError {
    fn from(error: UnknownMimeType) -> Self {
        Self::UnknownMimeType(error)
    }
}

impl From<metaflac::Error> for TaggingError {
    fn from(error: metaflac::Error) -> Self {
        Self::MetaflacError(error)
    }
}

impl RedbookError for TaggingError {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        io::ErrorKind::InvalidData
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownMimeType(Box<str>);

impl Error for UnknownMimeType {}

impl RedbookError for UnknownMimeType {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        io::ErrorKind::InvalidData
    }
}

impl Display for UnknownMimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for UnknownMimeType {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}

/// Leaf error: the provided tracks do not match the TOC (see [`Disc::new`][crate::Disc::new]).
#[derive(Debug, Clone, PartialEq)]
pub enum TocMismatch {
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

impl Display for TocMismatch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TocMismatch::Mismatch { provided, from_toc } => {
                write!(
                    f,
                    "track mismatch: toc {from_toc:?} vs provided track {provided}"
                )
            }
            TocMismatch::ExtraProvided(track) => write!(
                f,
                "provided tracks contain more entries than the toc; first additional entry {track}"
            ),
            TocMismatch::ExtraInToc(toc_track) => write!(
                f,
                "toc contains more entries than the provided tracks; first additional entry {toc_track:?}"
            ),
        }
    }
}

impl Error for TocMismatch {}

impl RedbookError for TocMismatch {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        io::ErrorKind::InvalidInput
    }
}

#[derive(Debug)]
pub enum IoError {
    /// Fetching disc metadata from MusicBrainz.
    ///
    /// Boxed because the snafu-generated error is too large to keep the `Err` variant
    /// small (`clippy::result_large_err`).
    MusicBrainz(Box<musicbrainz_rs::ApiEndpointError>),
    /// Fetching information directly, e.g. from downloading cover art.
    Http(reqwest::Error),
    /// Other cases, e.g. file system access
    StdIoError(io::Error),
}

impl From<musicbrainz_rs::ApiEndpointError> for IoError {
    fn from(error: musicbrainz_rs::ApiEndpointError) -> Self {
        Self::MusicBrainz(Box::new(error))
    }
}

impl From<reqwest::Error> for IoError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}

impl From<io::Error> for IoError {
    fn from(error: io::Error) -> Self {
        Self::StdIoError(error)
    }
}

impl Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IoError::MusicBrainz(_) => write!(f, "MusicBrainz API error"),
            IoError::Http(_) => write!(f, "HTTP error"),
            IoError::StdIoError(_) => write!(f, "I/O error"),
        }
    }
}

impl Error for IoError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            IoError::MusicBrainz(error) => Some(&**error),
            IoError::Http(error) => Some(error),
            IoError::StdIoError(error) => Some(error),
        }
    }
}

impl RedbookError for IoError {}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: Error + Send + Sync + 'static>() {}
        assert_traits::<DiscError>();
        assert_traits::<TocMismatch>();
        assert_traits::<TaggingError>();
        assert_traits::<IoError>();
    }

    #[test]
    fn wrapped_child_errors_are_source_not_display() {
        let toc_track = cdtoc::Toc::from_cdtoc("1+96+2D2B")
            .unwrap()
            .audio_track(1)
            .unwrap();
        let error = DiscError::from(TocMismatch::ExtraInToc(toc_track));
        assert_eq!(error.to_string(), "provided tracks do not match the TOC");
        let source = Error::source(&error).expect("wrapped TocMismatch should be the source");
        assert!(source.downcast_ref::<TocMismatch>().is_some());

        let error = DiscError::from(UnknownMimeType::from("application/x-unknown"));
        assert_eq!(error.to_string(), "tagging error");
        let tagging = Error::source(&error)
            .and_then(|s| s.downcast_ref::<TaggingError>())
            .expect("source should be the TaggingError");
        assert_eq!(tagging.to_string(), "unknown MIME type");
        assert!(
            Error::source(tagging)
                .and_then(|s| s.downcast_ref::<UnknownMimeType>())
                .is_some()
        );

        let error = DiscError::from(io::Error::from_raw_os_error(5));
        assert_eq!(error.to_string(), "I/O error");
        let io = Error::source(&error)
            .and_then(|s| s.downcast_ref::<IoError>())
            .expect("source should be the disc IoError");
        assert_eq!(io.to_string(), "I/O error");
        assert_eq!(
            Error::source(io)
                .and_then(|s| s.downcast_ref::<io::Error>())
                .unwrap()
                .raw_os_error(),
            Some(5)
        );

        assert!(Error::source(&DiscError::NoReleases).is_none());
    }

    #[cfg(false)]
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

    #[cfg(false)]
    #[test]
    fn source_returns_wrapped_errors_only() {
        let error = DiscError::from(io::Error::from_raw_os_error(5));
        assert!(
            Error::source(&error)
                .and_then(|s| s.downcast_ref::<io::Error>())
                .is_some()
        );
        assert!(Error::source(&DiscError::NoReleases).is_none());
    }

    #[cfg(false)]
    #[test]
    fn converts_into_root_error() {
        let error = Error::from(DiscError::NoReleases);
        assert!(matches!(error, Error::Disc(DiscError::NoReleases)));
    }
}
