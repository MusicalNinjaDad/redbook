//! Crate-root error type & [`Result`] alias: level 1 of redbook's error hierarchy.
//!
//! # Hierarchy
//!
//! Errors form a 2-3 level tree:
//!
//! 1. [`Error`] - the crate-root error, re-exported as [`redbook::Error`][crate::Error]. Used by
//!    public APIs which span multiple domains; re-exported [`Result`] alias defaults to it.
//! 2. Domain errors (e.g. `toc::TocError`, `drive::DriveError`) - one per module, following the
//!    same `struct { kind: KindEnum, context }` shape as [`Error`]. Most public signatures return
//!    `Result<_, DomainError>` directly.
//! 3. Leaf kind variants - enum variants of each `*ErrorKind`, which may hold a wrapped foreign
//!    error in a single tuple field (e.g. `Io(io::Error)`, `Metaflac(metaflac::Error)`).
//!
//! # Conventions
//!
//! - All error types implement [`std::error::Error`], are `Send + Sync + 'static` (C-GOOD-ERR) and
//!   `#[non_exhaustive]`.
//! - `source()` always returns the *directly wrapped* type: each level returns its immediate inner
//!   error, so drilling down via `source()` walks the whole chain (a wrapped `io::Error` keeps its
//!   original [`io::ErrorKind`] / raw OS error all the way down).
//! - Every error type implements `From<This> for io::Error` for interop; the [`io::ErrorKind`] is
//!   chosen to match the call site's previous behaviour, and the original error remains available
//!   via [`io::Error::get_ref`].
//! - Simple variants provide a constructor method of the same name so they can be used directly in
//!   `.map_err(Error::variant)`; optional context is attached via chaining `.context(...)` and is
//!   omitted from [`Display`] output when not provided.
use std::{
    error::Error as StdError,
    fmt::{self, Debug, Display},
    io,
};

/// [`std::result::Result`] alias which defaults to the crate-root [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Crate-root error: level 1 of the error hierarchy.
///
/// Construct via the variant constructors (e.g. [`Error::io`]), via `From` conversions from domain
/// errors or `io::Error`, or via `?`. Attach optional context with [`Error::context`].
#[derive(Debug)]
#[non_exhaustive]
// TODO: #192 flatten redbook::Error to enum
pub struct Error {
    kind: ErrorKind,
    context: Option<Box<str>>,
}

impl Error {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &ErrorKind {
        &self.kind
    }

    /// Attach optional context, returned by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// An [`io::Error`] occurred (level-3 leaf wrapped at the crate root).
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: ErrorKind::Io(error),
            context: None,
        }
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> io::ErrorKind {
        match &self.kind {
            ErrorKind::Io(error) => error.kind(),
            ErrorKind::Toc(error) => error.io_kind(),
            ErrorKind::Disc(error) => error.io_kind(),
            ErrorKind::Tagging(error) => error.io_kind(),
            ErrorKind::Track(error) => error.io_kind(),
            ErrorKind::Drive(error) => error.io_kind(),
            ErrorKind::Win(error) => error.io_kind(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<ErrorKind> for Error {
    fn from(kind: ErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// The kind of a crate-root [`Error`]: level 2/3 of the hierarchy.
///
/// Domain variants are added as each module migrates onto the hierarchy.
#[derive(Debug)]
#[non_exhaustive]
pub enum ErrorKind {
    /// A wrapped [`io::Error`], e.g. from the OS or the filesystem.
    Io(io::Error),
    /// A [`toc`][crate::toc] domain error.
    Toc(crate::toc::TocError),
    /// A [`disc`][crate::disc] domain error.
    Disc(crate::disc::DiscError),
    /// A [`tagging`][crate::tagging] domain error.
    Tagging(crate::tagging::TaggingError),
    /// A track-encoding domain error.
    Track(crate::track::TrackError),
    /// A [`drive`][crate::drive] domain error.
    Drive(crate::drive::DriveError),
    /// A [`win`] domain error (Windows hardware access).
    Win(crate::win::WinError),
}

impl Display for ErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErrorKind::Io(error) => Display::fmt(error, f),
            ErrorKind::Toc(error) => Display::fmt(error, f),
            ErrorKind::Disc(error) => Display::fmt(error, f),
            ErrorKind::Tagging(error) => Display::fmt(error, f),
            ErrorKind::Track(error) => Display::fmt(error, f),
            ErrorKind::Drive(error) => Display::fmt(error, f),
            ErrorKind::Win(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for ErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            ErrorKind::Io(error) => Some(error),
            ErrorKind::Toc(error) => Some(error),
            ErrorKind::Disc(error) => Some(error),
            ErrorKind::Tagging(error) => Some(error),
            ErrorKind::Track(error) => Some(error),
            ErrorKind::Drive(error) => Some(error),
            ErrorKind::Win(error) => Some(error),
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
        assert_traits::<Error>();
        assert_traits::<ErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        let error = Error::io(io::Error::new(io::ErrorKind::NotFound, "no disc"));
        assert_eq!(error.to_string(), "no disc");
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error =
            Error::io(io::Error::new(io::ErrorKind::NotFound, "no disc")).context("opening drive");
        assert_eq!(error.to_string(), "no disc: opening drive");
    }

    #[test]
    fn source_returns_directly_wrapped_io_error() {
        let raw = io::Error::from_raw_os_error(22); // ERROR_NOT_READY
        let error = Error::from(raw);
        let source = StdError::source(&error).expect("kind should be the source");
        let source = source
            .downcast_ref::<ErrorKind>()
            .expect("source should be the kind");
        let inner = StdError::source(source).expect("kind should wrap the io::Error");
        let inner = inner
            .downcast_ref::<io::Error>()
            .expect("innermost should be the original io::Error");
        assert_eq!(inner.raw_os_error(), Some(22));
    }

    #[test]
    fn io_error_conversion_preserves_kind_and_downcast() {
        let raw = io::Error::from_raw_os_error(22); // ERROR_NOT_READY on Windows
        let expected_kind = raw.kind();
        let io_error = io::Error::from(Error::from(raw));
        assert_eq!(io_error.kind(), expected_kind);
        let payload = io_error.get_ref().expect("io::Error should hold a payload");
        let error = payload
            .downcast_ref::<Error>()
            .expect("payload should be the original Error");
        let kind = StdError::source(error)
            .and_then(StdError::source)
            .and_then(|kind| kind.downcast_ref::<io::Error>())
            .expect("source chain should reach the original io::Error");
        assert_eq!(kind.raw_os_error(), Some(22));
    }
}
