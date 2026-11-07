//! Level-2 error for [`toc`][crate::toc]: [`TocError`] with leaf kinds in [`TocErrorKind`].
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `#[non_exhaustive]`, optional `.context(...)` chaining which is
//! omitted from `Display` when absent, `source()` returning the directly wrapped type, and
//! `From<TocError> for io::Error` preserving the call site's previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use crate::{
    error::{Error, ErrorKind},
    toc::RawTocEntry,
};

pub(super) type TocResult<T> = Result<T, TocError>;

/// Error returned by [`toc`][crate::toc] APIs: TOC parsing & validation failures.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(TocError::invalid_data)`) and attach optional context with
/// [`TocError::context`]. Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
// TODO: Delete Me!
pub struct TocErrorOld {
    kind: TocError,
    // TODO: #193 store semantically meaningful context
    context: Option<Box<str>>,
}

impl TocErrorOld {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &TocError {
        &self.kind
    }

    /// Attach optional context, rendered by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// The TOC data is invalid / inconsistent.
    pub const fn invalid_data() -> Self {
        Self {
            kind: TocError::InvalidData,
            context: None,
        }
    }

    /// A required TOC element (e.g. the lead-out) was not found.
    pub const fn not_found() -> Self {
        Self {
            kind: TocError::NotFound,
            context: None,
        }
    }

    /// A wrapped [`io::Error`].
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: TocError::Io(error),
            context: None,
        }
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            TocError::InvalidData | TocError::CdToc(_) => IoErrorKind::InvalidData,
            TocError::NotFound => IoErrorKind::NotFound,
            TocError::Io(error) => error.kind(),
            _ => todo!(),
        }
    }
}

impl Display for TocErrorOld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for TocErrorOld {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<TocError> for TocErrorOld {
    fn from(kind: TocError) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<cdtoc::TocError> for TocErrorOld {
    fn from(error: cdtoc::TocError) -> Self {
        Self {
            kind: TocError::CdToc(error),
            context: None,
        }
    }
}

impl From<io::Error> for TocErrorOld {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<TocErrorOld> for Error {
    fn from(error: TocErrorOld) -> Self {
        Error::from(ErrorKind::Toc(error))
    }
}

impl From<TocErrorOld> for io::Error {
    fn from(error: TocErrorOld) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Interop downgrade to [`cdtoc`]'s error type: a wrapped [`cdtoc::TocError`] is simply
/// unwrapped; other kinds map to the closest `cdtoc` semantic (`SectorOrder`).
impl From<TocErrorOld> for cdtoc::TocError {
    fn from(error: TocErrorOld) -> Self {
        match error.kind {
            TocError::CdToc(error) => error,
            TocError::InvalidData | TocError::NotFound | TocError::Io(_) => {
                cdtoc::TocError::SectorOrder
            }
            _ => todo!(),
        }
    }
}

/// Leaf kinds of [`TocError`]: level 3 of the hierarchy. Variants wrapping a foreign error hold
/// it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
pub enum TocError {
    /// Attempt to createa a TrackEntry from invalid input
    InvalidTrackEntry(RawTocEntry),
    /// Track number outside valid range
    InvalidTrackNo(InvalidTrackNo),
    /// The TOC data is invalid / inconsistent.
    InvalidData,
    /// A required TOC element (e.g. the lead-out) was not found.
    NotFound,
    /// A wrapped [`cdtoc::TocError`], e.g. from [`cdtoc::Toc::from_parts`].
    CdToc(cdtoc::TocError),
    /// A wrapped [`io::Error`].
    Io(io::Error),
}

impl From<InvalidTrackNo> for TocError {
    fn from(error: InvalidTrackNo) -> Self {
        TocError::InvalidTrackNo(error)
    }
}

impl Display for TocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TocError::InvalidTrackEntry(raw) => write!(f, "entry not valid as a track: {raw:?}"),
            TocError::InvalidTrackNo(error) => error.fmt(f),
            TocError::InvalidData => f.write_str("invalid TOC data"),
            TocError::NotFound => f.write_str("required TOC element not found"),
            TocError::CdToc(error) => Display::fmt(error, f),
            TocError::Io(error) => Display::fmt(error, f),
        }
    }
}

// TODO #198 Either return source, or render in Display, not both
impl StdError for TocError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            // In error types that wrap an underlying error, the underlying error should be either
            // returned by the outer error’s Error::source(), or rendered by the outer error’s
            // Display implementation, but not both.
            // TODO: DELETE ABOVE COMMENT, only keep short line comment below
            TocError::InvalidTrackEntry(_) | TocError::InvalidTrackNo(_) => None, // rendered in Display
            TocError::CdToc(error) => Some(error),
            TocError::Io(error) => Some(error),
            TocError::InvalidData | TocError::NotFound => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
/// TODO - Half done example shape
pub struct InvalidTrackNo(i128);

impl InvalidTrackNo {
    /// TODO - Half done example shape
    pub fn new<N>(attempt: N) -> Self
    where
        i128: From<N>,
    {
        Self(attempt.into())
    }
}

impl StdError for InvalidTrackNo {}
impl Display for InvalidTrackNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid track number: {p:#04X}", p = self.0)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<TocErrorOld>();
        assert_traits::<TocError>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(TocErrorOld::invalid_data().to_string(), "invalid TOC data");
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = TocErrorOld::not_found().context("No leadout found");
        assert_eq!(
            error.to_string(),
            "required TOC element not found: No leadout found"
        );
    }

    #[test]
    fn source_returns_directly_wrapped_error() {
        let error = TocErrorOld::from(cdtoc::TocError::NoAudio);
        let source = StdError::source(&error).expect("kind should be the source");
        let kind = source
            .downcast_ref::<TocError>()
            .expect("source should be the kind");
        let inner = StdError::source(kind).expect("kind should wrap the cdtoc error");
        assert!(inner.downcast_ref::<cdtoc::TocError>().is_some());
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(TocErrorOld::invalid_data()).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(TocErrorOld::not_found()).kind(),
            IoErrorKind::NotFound
        );
        assert_eq!(
            io::Error::from(TocErrorOld::from(cdtoc::TocError::NoAudio)).kind(),
            IoErrorKind::InvalidData
        );
        let raw = io::Error::from_raw_os_error(22);
        let io_error = io::Error::from(TocErrorOld::from(raw));
        let payload = io_error.get_ref().expect("io::Error should hold a payload");
        let error = payload
            .downcast_ref::<TocErrorOld>()
            .expect("payload should be the original TocError");
        let source = StdError::source(error)
            .and_then(StdError::source)
            .and_then(|kind| kind.downcast_ref::<io::Error>())
            .expect("source chain should reach the original io::Error");
        assert_eq!(source.raw_os_error(), Some(22));
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(TocErrorOld::invalid_data().context("bad entry"));
        assert!(matches!(error.kind(), ErrorKind::Toc(_)));
        assert_eq!(error.to_string(), "invalid TOC data: bad entry");
    }

    #[test]
    fn downgrades_to_cdtoc_error_by_unwrapping() {
        let wrapped = TocErrorOld::from(cdtoc::TocError::TrackCount);
        assert_eq!(cdtoc::TocError::from(wrapped), cdtoc::TocError::TrackCount);
        assert_eq!(
            cdtoc::TocError::from(TocErrorOld::invalid_data()),
            cdtoc::TocError::SectorOrder
        );
    }
}
