//! Level-2 error for [`drive`][crate::drive] (the [`Device`]/[`Drive`] traits): the
//! [`DriveError`] enum.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error, data-carrying
//! variants rendered by `Display`, and `From<DriveError> for io::Error` preserving the call
//! site's previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use thread_safely::Context;

use crate::{
    error::Error,
    frame::ByteSize,
    toc::{TocError, TrackNo},
    win::WinError,
};

/// Error returned by [`Device`][crate::Device] / [`Drive`][crate::Drive] APIs.
///
/// Variants carry the data which led to the error (issue #193). Variants wrapping an underlying
/// error are returned by `source()` and *not* rendered by `Display` (issue #198). Wrapped foreign
/// errors convert via `From` / `?`.
#[derive(Debug)]
pub enum DriveError {
    /// The requested track was not found on the disc; carries the track number.
    NotFound(TrackNo),
    /// The track has zero duration / fewer bytes were read than expected.
    UnexpectedEof,
    /// The track is too long to buffer in memory; carries the track's size in bytes.
    FileTooLarge(ByteSize),
    /// The data or arguments are invalid, e.g. frame offsets overflowing CD max.
    InvalidData,
    /// The operation is not supported, e.g. a buffer not sized for an exact number of frames.
    Unsupported,
    /// The buffer is too large; carries the attempted buffer size in bytes.
    OutOfMemory(usize),
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

impl DriveError {
    /// Whether the underlying OS error is `ERROR_NOT_READY` (no disc in drive).
    pub fn is_not_ready(&self) -> bool {
        matches!(self, DriveError::Win(error) if error.is_not_ready())
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match self {
            DriveError::NotFound(_) => IoErrorKind::NotFound,
            DriveError::UnexpectedEof => IoErrorKind::UnexpectedEof,
            DriveError::FileTooLarge(_) => IoErrorKind::FileTooLarge,
            DriveError::InvalidData => IoErrorKind::InvalidData,
            DriveError::Unsupported => IoErrorKind::Unsupported,
            DriveError::OutOfMemory(_) => IoErrorKind::OutOfMemory,
            DriveError::Cancelled => IoErrorKind::Interrupted,
            DriveError::Progress => IoErrorKind::Other,
            DriveError::Io(error) => error.kind(),
            DriveError::Win(error) => error.io_kind(),
            DriveError::Toc(error) => error.io_kind(),
        }
    }
}

impl Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveError::NotFound(track_number) => {
                write!(f, "track {} not found on disc", track_number.cast::<u8>())
            }
            DriveError::UnexpectedEof => f.write_str("unexpected end of data"),
            DriveError::FileTooLarge(size) => {
                write!(f, "track too long to buffer in memory: {size}")
            }
            DriveError::InvalidData => f.write_str("invalid data"),
            DriveError::Unsupported => f.write_str("operation not supported"),
            DriveError::OutOfMemory(size) => write!(f, "buffer too large: {size} bytes"),
            DriveError::Cancelled => f.write_str("thread cancellation requested by controller"),
            DriveError::Progress => f.write_str("failed to send progress update"),
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            DriveError::Io(_) => f.write_str("I/O error"),
            DriveError::Win(_) => f.write_str("OS / hardware error"),
            DriveError::Toc(_) => f.write_str("TOC error"),
        }
    }
}

impl StdError for DriveError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DriveError::Io(error) => Some(error),
            DriveError::Win(error) => Some(error),
            DriveError::Toc(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            DriveError::NotFound(_)
            | DriveError::UnexpectedEof
            | DriveError::FileTooLarge(_)
            | DriveError::InvalidData
            | DriveError::Unsupported
            | DriveError::OutOfMemory(_)
            | DriveError::Cancelled
            | DriveError::Progress => None,
        }
    }
}

impl From<io::Error> for DriveError {
    fn from(error: io::Error) -> Self {
        DriveError::Io(error)
    }
}

impl<R> From<Context<R>> for DriveError {
    fn from(_: Context<R>) -> Self {
        DriveError::Cancelled
    }
}

impl From<WinError> for DriveError {
    fn from(error: WinError) -> Self {
        DriveError::Win(error)
    }
}

impl From<TocError> for DriveError {
    fn from(error: TocError) -> Self {
        DriveError::Toc(error)
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

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<DriveError>();
    }

    #[test]
    fn display_renders_labels_and_data() {
        assert_eq!(
            DriveError::NotFound(TrackNo::strict_new(3)).to_string(),
            "track 3 not found on disc"
        );
        assert_eq!(
            DriveError::from(io::Error::new(IoErrorKind::NotFound, "gone")).to_string(),
            "I/O error"
        );
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(DriveError::NotFound(TrackNo::strict_new(1))).kind(),
            IoErrorKind::NotFound
        );
        assert_eq!(
            io::Error::from(DriveError::UnexpectedEof).kind(),
            IoErrorKind::UnexpectedEof
        );
        assert_eq!(
            io::Error::from(DriveError::FileTooLarge(ByteSize::MAX)).kind(),
            IoErrorKind::FileTooLarge
        );
        assert_eq!(
            io::Error::from(DriveError::InvalidData).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(DriveError::Unsupported).kind(),
            IoErrorKind::Unsupported
        );
        assert_eq!(
            io::Error::from(DriveError::OutOfMemory(1024)).kind(),
            IoErrorKind::OutOfMemory
        );
        assert_eq!(
            io::Error::from(DriveError::Cancelled).kind(),
            IoErrorKind::Interrupted
        );
    }

    #[test]
    fn source_returns_directly_wrapped_win_error() {
        let win = WinError::NotReady;
        let error = DriveError::from(win);
        assert!(error.is_not_ready());
        let source = StdError::source(&error).expect("wrapped win error should be the source");
        let win = source
            .downcast_ref::<WinError>()
            .expect("source should be the WinError directly");
        assert!(win.is_not_ready());
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(DriveError::UnexpectedEof);
        assert!(matches!(error, Error::Drive(DriveError::UnexpectedEof)));
    }
}
