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

use crate::error::{Error, ErrorKind};

/// Error returned by [`toc`][crate::toc] APIs: TOC parsing & validation failures.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(TocError::invalid_data)`) and attach optional context with
/// [`TocError::context`]. Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct TocError {
    kind: TocErrorKind,
    context: Option<Box<str>>,
}

impl TocError {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &TocErrorKind {
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
            kind: TocErrorKind::InvalidData,
            context: None,
        }
    }

    /// A required TOC element (e.g. the lead-out) was not found.
    pub const fn not_found() -> Self {
        Self {
            kind: TocErrorKind::NotFound,
            context: None,
        }
    }

    /// A wrapped [`io::Error`].
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: TocErrorKind::Io(error),
            context: None,
        }
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            TocErrorKind::InvalidData | TocErrorKind::CdToc(_) => IoErrorKind::InvalidData,
            TocErrorKind::NotFound => IoErrorKind::NotFound,
            TocErrorKind::Io(error) => error.kind(),
        }
    }
}

impl Display for TocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for TocError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<TocErrorKind> for TocError {
    fn from(kind: TocErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<cdtoc::TocError> for TocError {
    fn from(error: cdtoc::TocError) -> Self {
        Self {
            kind: TocErrorKind::CdToc(error),
            context: None,
        }
    }
}

impl From<io::Error> for TocError {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<TocError> for Error {
    fn from(error: TocError) -> Self {
        Error::from(ErrorKind::Toc(error))
    }
}

impl From<TocError> for io::Error {
    fn from(error: TocError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Interop downgrade to [`cdtoc`]'s error type: a wrapped [`cdtoc::TocError`] is simply
/// unwrapped; other kinds map to the closest `cdtoc` semantic (`SectorOrder`).
impl From<TocError> for cdtoc::TocError {
    fn from(error: TocError) -> Self {
        match error.kind {
            TocErrorKind::CdToc(error) => error,
            TocErrorKind::InvalidData | TocErrorKind::NotFound | TocErrorKind::Io(_) => {
                cdtoc::TocError::SectorOrder
            }
        }
    }
}

/// Leaf kinds of [`TocError`]: level 3 of the hierarchy. Variants wrapping a foreign error hold
/// it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum TocErrorKind {
    /// The TOC data is invalid / inconsistent.
    InvalidData,
    /// A required TOC element (e.g. the lead-out) was not found.
    NotFound,
    /// A wrapped [`cdtoc::TocError`], e.g. from [`cdtoc::Toc::from_parts`].
    CdToc(cdtoc::TocError),
    /// A wrapped [`io::Error`].
    Io(io::Error),
}

impl Display for TocErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TocErrorKind::InvalidData => f.write_str("invalid TOC data"),
            TocErrorKind::NotFound => f.write_str("required TOC element not found"),
            TocErrorKind::CdToc(error) => Display::fmt(error, f),
            TocErrorKind::Io(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for TocErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TocErrorKind::CdToc(error) => Some(error),
            TocErrorKind::Io(error) => Some(error),
            TocErrorKind::InvalidData | TocErrorKind::NotFound => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<TocError>();
        assert_traits::<TocErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(TocError::invalid_data().to_string(), "invalid TOC data");
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = TocError::not_found().context("No leadout found");
        assert_eq!(
            error.to_string(),
            "required TOC element not found: No leadout found"
        );
    }

    #[test]
    fn source_returns_directly_wrapped_error() {
        let error = TocError::from(cdtoc::TocError::NoAudio);
        let source = StdError::source(&error).expect("kind should be the source");
        let kind = source
            .downcast_ref::<TocErrorKind>()
            .expect("source should be the kind");
        let inner = StdError::source(kind).expect("kind should wrap the cdtoc error");
        assert!(inner.downcast_ref::<cdtoc::TocError>().is_some());
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(TocError::invalid_data()).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(TocError::not_found()).kind(),
            IoErrorKind::NotFound
        );
        assert_eq!(
            io::Error::from(TocError::from(cdtoc::TocError::NoAudio)).kind(),
            IoErrorKind::InvalidData
        );
        let raw = io::Error::from_raw_os_error(22);
        let io_error = io::Error::from(TocError::from(raw));
        let payload = io_error.get_ref().expect("io::Error should hold a payload");
        let error = payload
            .downcast_ref::<TocError>()
            .expect("payload should be the original TocError");
        let source = StdError::source(error)
            .and_then(StdError::source)
            .and_then(|kind| kind.downcast_ref::<io::Error>())
            .expect("source chain should reach the original io::Error");
        assert_eq!(source.raw_os_error(), Some(22));
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(TocError::invalid_data().context("bad entry"));
        assert!(matches!(error.kind(), ErrorKind::Toc(_)));
        assert_eq!(error.to_string(), "invalid TOC data: bad entry");
    }

    #[test]
    fn downgrades_to_cdtoc_error_by_unwrapping() {
        let wrapped = TocError::from(cdtoc::TocError::TrackCount);
        assert_eq!(cdtoc::TocError::from(wrapped), cdtoc::TocError::TrackCount);
        assert_eq!(
            cdtoc::TocError::from(TocError::invalid_data()),
            cdtoc::TocError::SectorOrder
        );
    }
}
