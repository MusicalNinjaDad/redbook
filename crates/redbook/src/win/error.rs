//! Level-2 error for the [`win`] module (Windows hardware access):
//! [`WinError`] with leaf kinds in [`WinErrorKind`].
//!
//! Follows the crate-wide conventions documented in [`crate::error`]. In addition, OS errors get
//! dedicated handling: a wrapped [`io::Error`] from the OS is returned directly from
//! `From<WinError> for io::Error` (preserving `raw_os_error`), and `ERROR_NOT_READY` (21, no disc
//! in drive) is surfaced as the dedicated [`WinErrorKind::NotReady`] variant.
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use crate::{error::Error, toc::TocError};

/// `ERROR_NOT_READY`: the device is not ready, e.g. no disc in the drive.
pub const ERROR_NOT_READY: i32 = 21;

/// Error returned by [`win`] APIs: OS/hardware failures and `.cda` file parsing failures.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(WinError::invalid_data)`) and attach optional context with [`WinError::context`].
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct WinError {
    kind: WinErrorKind,
    context: Option<Box<str>>,
}

impl WinError {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &WinErrorKind {
        &self.kind
    }

    /// Attach optional context, rendered by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// An OS / [`io::Error`] occurred. `ERROR_NOT_READY` (21) is mapped to
    /// [`WinErrorKind::NotReady`].
    pub fn io(error: io::Error) -> Self {
        if error.raw_os_error() == Some(ERROR_NOT_READY) {
            Self::not_ready()
        } else {
            Self {
                kind: WinErrorKind::Io(error),
                context: None,
            }
        }
    }

    /// The device is not ready, e.g. no disc in the drive (`ERROR_NOT_READY`, 21).
    pub const fn not_ready() -> Self {
        Self {
            kind: WinErrorKind::NotReady,
            context: None,
        }
    }

    /// The data is invalid.
    pub const fn invalid_data() -> Self {
        Self {
            kind: WinErrorKind::InvalidData,
            context: None,
        }
    }

    /// The filename/path is invalid.
    pub const fn invalid_filename() -> Self {
        Self {
            kind: WinErrorKind::InvalidFilename,
            context: None,
        }
    }

    /// The operation is not supported by the device.
    pub const fn unsupported() -> Self {
        Self {
            kind: WinErrorKind::Unsupported,
            context: None,
        }
    }

    /// The buffer is too large.
    pub const fn out_of_memory() -> Self {
        Self {
            kind: WinErrorKind::OutOfMemory,
            context: None,
        }
    }

    /// Fewer bytes were returned than expected.
    pub const fn unexpected_eof() -> Self {
        Self {
            kind: WinErrorKind::UnexpectedEof,
            context: None,
        }
    }

    /// Whether this error is `ERROR_NOT_READY` (no disc in drive).
    pub fn is_not_ready(&self) -> bool {
        matches!(self.kind, WinErrorKind::NotReady)
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            WinErrorKind::Io(error) => error.kind(),
            WinErrorKind::NotReady => io::Error::from_raw_os_error(ERROR_NOT_READY).kind(),
            WinErrorKind::InvalidData | WinErrorKind::Toc(_) => IoErrorKind::InvalidData,
            WinErrorKind::InvalidFilename => IoErrorKind::InvalidFilename,
            WinErrorKind::Unsupported => IoErrorKind::Unsupported,
            WinErrorKind::OutOfMemory => IoErrorKind::OutOfMemory,
            WinErrorKind::UnexpectedEof => IoErrorKind::UnexpectedEof,
        }
    }
}

impl Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for WinError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<WinErrorKind> for WinError {
    fn from(kind: WinErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<io::Error> for WinError {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<TocError> for WinError {
    fn from(error: TocError) -> Self {
        Self {
            kind: WinErrorKind::Toc(error),
            context: None,
        }
    }
}

impl From<WinError> for Error {
    fn from(error: WinError) -> Self {
        crate::error::ErrorKind::Win(error).into()
    }
}

impl From<WinError> for io::Error {
    fn from(error: WinError) -> Self {
        match error.kind {
            // return the original OS error directly, preserving raw_os_error (status quo: call
            // sites propagated the io::Error unchanged)
            WinErrorKind::Io(error) => error,
            WinErrorKind::NotReady => io::Error::from_raw_os_error(ERROR_NOT_READY),
            kind => io::Error::new(
                match &kind {
                    WinErrorKind::InvalidData | WinErrorKind::Toc(_) => IoErrorKind::InvalidData,
                    WinErrorKind::InvalidFilename => IoErrorKind::InvalidFilename,
                    WinErrorKind::Unsupported => IoErrorKind::Unsupported,
                    WinErrorKind::OutOfMemory => IoErrorKind::OutOfMemory,
                    WinErrorKind::UnexpectedEof => IoErrorKind::UnexpectedEof,
                    WinErrorKind::Io(_) | WinErrorKind::NotReady => unreachable!(),
                },
                WinError {
                    kind,
                    context: error.context,
                },
            ),
        }
    }
}

/// Leaf kinds of [`WinError`]: level 3 of the hierarchy. Variants wrapping a foreign error hold
/// it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum WinErrorKind {
    /// A wrapped [`io::Error`], e.g. a raw OS error from a Windows API call.
    Io(io::Error),
    /// The device is not ready, e.g. no disc in the drive (`ERROR_NOT_READY`, 21).
    NotReady,
    /// The data is invalid.
    InvalidData,
    /// The filename/path is invalid.
    InvalidFilename,
    /// The operation is not supported by the device.
    Unsupported,
    /// The buffer is too large.
    OutOfMemory,
    /// Fewer bytes were returned than expected.
    UnexpectedEof,
    /// A wrapped [`toc`][crate::toc] error, e.g. from parsing a raw TOC.
    Toc(TocError),
}

impl Display for WinErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WinErrorKind::Io(error) => Display::fmt(error, f),
            WinErrorKind::NotReady => f.write_str("device not ready (no disc in drive)"),
            WinErrorKind::InvalidData => f.write_str("invalid data"),
            WinErrorKind::InvalidFilename => f.write_str("invalid filename"),
            WinErrorKind::Unsupported => f.write_str("operation not supported"),
            WinErrorKind::OutOfMemory => f.write_str("buffer too large"),
            WinErrorKind::UnexpectedEof => f.write_str("unexpected end of data"),
            WinErrorKind::Toc(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for WinErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            WinErrorKind::Io(error) => Some(error),
            WinErrorKind::Toc(error) => Some(error),
            WinErrorKind::NotReady
            | WinErrorKind::InvalidData
            | WinErrorKind::InvalidFilename
            | WinErrorKind::Unsupported
            | WinErrorKind::OutOfMemory
            | WinErrorKind::UnexpectedEof => None,
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
        assert_traits::<WinError>();
        assert_traits::<WinErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(
            WinError::not_ready().to_string(),
            "device not ready (no disc in drive)"
        );
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = WinError::invalid_data().context("Missing or invalid RIFF header");
        assert_eq!(
            error.to_string(),
            "invalid data: Missing or invalid RIFF header"
        );
    }

    #[test]
    fn os_error_21_maps_to_not_ready() {
        let error = WinError::from(io::Error::from_raw_os_error(ERROR_NOT_READY));
        assert!(error.is_not_ready());
        assert!(matches!(error.kind(), WinErrorKind::NotReady));
        assert_eq!(
            io::Error::from(error).raw_os_error(),
            Some(ERROR_NOT_READY),
            "conversion back to io::Error preserves the raw OS error code"
        );
    }

    #[test]
    fn other_os_errors_keep_their_io_error() {
        let raw = io::Error::from_raw_os_error(5); // ERROR_ACCESS_DENIED
        let error = WinError::from(raw);
        assert!(matches!(error.kind(), WinErrorKind::Io(_)));
        let io_error = io::Error::from(error);
        assert_eq!(io_error.raw_os_error(), Some(5));
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(WinError::invalid_data()).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(WinError::invalid_filename()).kind(),
            IoErrorKind::InvalidFilename
        );
        assert_eq!(
            io::Error::from(WinError::unsupported()).kind(),
            IoErrorKind::Unsupported
        );
        assert_eq!(
            io::Error::from(WinError::out_of_memory()).kind(),
            IoErrorKind::OutOfMemory
        );
        assert_eq!(
            io::Error::from(WinError::unexpected_eof()).kind(),
            IoErrorKind::UnexpectedEof
        );
    }
}
