//! Level-2 error for [`drive`][crate::drive] (the [`Device`]/[`Drive`] traits):
//! [`DriveError`] with leaf kinds in [`DriveErrorKind`].
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `#[non_exhaustive]`, optional `.context(...)` chaining which is
//! omitted from `Display` when absent, `source()` returning the directly wrapped type, and
//! `From<DriveError> for io::Error` preserving the call site's previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use thread_safely::Context;

use crate::{error::Error, toc::TocError, win::WinError};

/// Error returned by [`Device`][crate::Device] / [`Drive`][crate::Drive] APIs.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(DriveError::not_found)`) and attach optional context with
/// [`DriveError::context`]. Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct DriveError {
    kind: DriveErrorKind,
    context: Option<Box<str>>,
}

impl DriveError {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &DriveErrorKind {
        &self.kind
    }

    /// Attach optional context, rendered by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// The requested track / element was not found.
    pub const fn not_found() -> Self {
        Self {
            kind: DriveErrorKind::NotFound,
            context: None,
        }
    }

    /// The track has zero duration / fewer bytes were read than expected.
    pub const fn unexpected_eof() -> Self {
        Self {
            kind: DriveErrorKind::UnexpectedEof,
            context: None,
        }
    }

    /// The track is too long to buffer in memory.
    pub const fn file_too_large() -> Self {
        Self {
            kind: DriveErrorKind::FileTooLarge,
            context: None,
        }
    }

    /// The data or arguments are invalid, e.g. frame offsets overflowing CD max.
    pub const fn invalid_data() -> Self {
        Self {
            kind: DriveErrorKind::InvalidData,
            context: None,
        }
    }

    /// The buffer is too large.
    pub const fn out_of_memory() -> Self {
        Self {
            kind: DriveErrorKind::OutOfMemory,
            context: None,
        }
    }

    /// The operation is not supported, e.g. a buffer not sized for an exact number of frames.
    pub const fn unsupported() -> Self {
        Self {
            kind: DriveErrorKind::Unsupported,
            context: None,
        }
    }

    /// Sending a progress update on the [`Context`][thread_safely::Context] channel failed.
    pub const fn progress() -> Self {
        Self {
            kind: DriveErrorKind::Progress,
            context: None,
        }
    }

    /// The operation was cancelled via its [`Context`].
    pub const fn cancelled() -> Self {
        Self {
            kind: DriveErrorKind::Cancelled,
            context: None,
        }
    }

    /// A wrapped [`io::Error`].
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: DriveErrorKind::Io(error),
            context: None,
        }
    }

    /// Whether the underlying OS error is `ERROR_NOT_READY` (no disc in drive).
    pub fn is_not_ready(&self) -> bool {
        matches!(&self.kind, DriveErrorKind::Win(error) if error.is_not_ready())
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            DriveErrorKind::NotFound => IoErrorKind::NotFound,
            DriveErrorKind::UnexpectedEof => IoErrorKind::UnexpectedEof,
            DriveErrorKind::FileTooLarge => IoErrorKind::FileTooLarge,
            DriveErrorKind::InvalidData => IoErrorKind::InvalidData,
            DriveErrorKind::Unsupported => IoErrorKind::Unsupported,
            DriveErrorKind::OutOfMemory => IoErrorKind::OutOfMemory,
            DriveErrorKind::Cancelled => IoErrorKind::Interrupted,
            DriveErrorKind::Progress => IoErrorKind::Other,
            DriveErrorKind::Io(error) => error.kind(),
            DriveErrorKind::Win(error) => error.io_kind(),
            DriveErrorKind::Toc(error) => error.io_kind(),
        }
    }
}

impl Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for DriveError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<DriveErrorKind> for DriveError {
    fn from(kind: DriveErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<io::Error> for DriveError {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl<R> From<Context<R>> for DriveError {
    fn from(_: Context<R>) -> Self {
        Self::cancelled()
    }
}

impl From<WinError> for DriveError {
    fn from(error: WinError) -> Self {
        Self {
            kind: DriveErrorKind::Win(error),
            context: None,
        }
    }
}

impl From<TocError> for DriveError {
    fn from(error: TocError) -> Self {
        Self {
            kind: DriveErrorKind::Toc(error),
            context: None,
        }
    }
}

impl From<DriveError> for Error {
    fn from(error: DriveError) -> Self {
        Error::Drive(error)
    }
}

impl From<DriveError> for io::Error {
    fn from(error: DriveError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Leaf kinds of [`DriveError`]: level 3 of the hierarchy. Variants wrapping a foreign error hold
/// it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum DriveErrorKind {
    /// The requested track / element was not found.
    NotFound,
    /// The track has zero duration / fewer bytes were read than expected.
    UnexpectedEof,
    /// The track is too long to buffer in memory.
    FileTooLarge,
    /// The data or arguments are invalid.
    InvalidData,
    /// The operation is not supported.
    Unsupported,
    /// The buffer is too large.
    OutOfMemory,
    /// The operation was cancelled via its [`Context`][thread_safely::Context].
    Cancelled,
    /// Sending a progress update on the [`Context`][thread_safely::Context] channel failed.
    Progress,
    /// A wrapped [`io::Error`].
    Io(io::Error),
    /// A wrapped [`win`][crate::win] (OS/hardware) error.
    Win(WinError),
    /// A wrapped [`toc`][crate::toc] error, e.g. from parsing the raw TOC.
    Toc(TocError),
}

impl Display for DriveErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveErrorKind::NotFound => f.write_str("not found"),
            DriveErrorKind::UnexpectedEof => f.write_str("unexpected end of data"),
            DriveErrorKind::FileTooLarge => f.write_str("track too long to buffer in memory"),
            DriveErrorKind::InvalidData => f.write_str("invalid data"),
            DriveErrorKind::Unsupported => f.write_str("operation not supported"),
            DriveErrorKind::OutOfMemory => f.write_str("buffer too large"),
            DriveErrorKind::Cancelled => f.write_str("thread cancellation requested by controller"),
            DriveErrorKind::Progress => f.write_str("failed to send progress update"),
            DriveErrorKind::Io(error) => Display::fmt(error, f),
            DriveErrorKind::Win(error) => Display::fmt(error, f),
            DriveErrorKind::Toc(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for DriveErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DriveErrorKind::Io(error) => Some(error),
            DriveErrorKind::Win(error) => Some(error),
            DriveErrorKind::Toc(error) => Some(error),
            DriveErrorKind::NotFound
            | DriveErrorKind::UnexpectedEof
            | DriveErrorKind::FileTooLarge
            | DriveErrorKind::InvalidData
            | DriveErrorKind::Unsupported
            | DriveErrorKind::OutOfMemory
            | DriveErrorKind::Cancelled
            | DriveErrorKind::Progress => None,
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
        assert_traits::<DriveError>();
        assert_traits::<DriveErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(DriveError::not_found().to_string(), "not found");
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = DriveError::not_found().context("invalid track number");
        assert_eq!(error.to_string(), "not found: invalid track number");
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(DriveError::not_found()).kind(),
            IoErrorKind::NotFound
        );
        assert_eq!(
            io::Error::from(DriveError::unexpected_eof()).kind(),
            IoErrorKind::UnexpectedEof
        );
        assert_eq!(
            io::Error::from(DriveError::file_too_large()).kind(),
            IoErrorKind::FileTooLarge
        );
        assert_eq!(
            io::Error::from(DriveError::invalid_data()).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(DriveError::unsupported()).kind(),
            IoErrorKind::Unsupported
        );
        assert_eq!(
            io::Error::from(DriveError::cancelled()).kind(),
            IoErrorKind::Interrupted
        );
    }

    #[test]
    fn source_returns_directly_wrapped_win_error() {
        let win = WinError::NotReady;
        let error = DriveError::from(win);
        assert!(error.is_not_ready());
        let source = StdError::source(&error).expect("kind should be the source");
        let kind = source
            .downcast_ref::<DriveErrorKind>()
            .expect("source should be the kind");
        let win = StdError::source(kind)
            .and_then(|k| k.downcast_ref::<WinError>())
            .expect("kind should wrap the WinError");
        assert!(win.is_not_ready());
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(DriveError::not_found());
        assert!(matches!(error, Error::Drive(_)));
    }
}
