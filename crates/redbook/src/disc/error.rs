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

impl RedbookError for DiscError {}

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

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            DiscError::TocMismatch(_) => {
                request.provide_value(io::ErrorKind::InvalidInput);
            }
            DiscError::TaggingError(_) => {
                // delegate
            }
            DiscError::NoReleases => {
                request.provide_value(io::ErrorKind::NotFound);
            }
            DiscError::ThreadCancellation => {
                request.provide_value(io::ErrorKind::Interrupted);
            }
            DiscError::IoError(_) => {
                // delegate
            }
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

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            TaggingError::UnknownMimeType(_) => {
                request.provide_value(io::ErrorKind::InvalidData);
            }
            TaggingError::MetaflacError(error) => match &error.kind {
                metaflac::ErrorKind::Io(error) => {
                    request.provide_value(error.kind());
                }
                metaflac::ErrorKind::StringDecoding(_) => {
                    request.provide_value(io::ErrorKind::InvalidData);
                }
                metaflac::ErrorKind::InvalidInput => {
                    request.provide_value(io::ErrorKind::InvalidInput);
                }
            },
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

impl RedbookError for TaggingError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownMimeType(Box<str>);

impl Error for UnknownMimeType {
    // no provide - this could be InvalidData or InvalidInput
}

impl RedbookError for UnknownMimeType {}

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

impl Error for TocMismatch {
    // no provide - could be InvalidData or InvalidInput
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

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            IoError::MusicBrainz(_) => {
                request.provide_value(io::ErrorKind::Other); // no access to relevant details
            }
            IoError::Http(_) => {
                request.provide_value(io::ErrorKind::Other); // no access to relevant details via match
            }
            IoError::StdIoError(error) => {
                request.provide_value(error.kind());
            }
        }
    }
}

impl RedbookError for IoError {}
