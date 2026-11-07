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

use crate::{
    error::Error,
    toc::{TocErrorOld, error::InvalidTrackNo},
};

/// `ERROR_NOT_READY`: the device is not ready, e.g. no disc in the drive.
pub const ERROR_NOT_READY: i32 = 21;

/// Error returned by [`win`] APIs: OS/hardware failures and `.cda` file parsing failures.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(WinError::invalid_data)`) and attach optional context with [`WinError::context`].
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct WinErrorOld {
    kind: WinError,
    context: Option<Box<str>>,
}

impl WinErrorOld {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &WinError {
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
                kind: WinError::Io(error),
                context: None,
            }
        }
    }

    /// The device is not ready, e.g. no disc in the drive (`ERROR_NOT_READY`, 21).
    pub const fn not_ready() -> Self {
        Self {
            kind: WinError::NotReady,
            context: None,
        }
    }

    /// The data is invalid.
    pub const fn invalid_data() -> Self {
        Self {
            kind: WinError::InvalidData,
            context: None,
        }
    }

    /// The filename/path is invalid.
    pub const fn invalid_filename() -> Self {
        Self {
            kind: WinError::InvalidFilename,
            context: None,
        }
    }

    /// The operation is not supported by the device.
    pub const fn unsupported() -> Self {
        Self {
            kind: WinError::Unsupported,
            context: None,
        }
    }

    /// The buffer is too large.
    pub const fn out_of_memory() -> Self {
        Self {
            kind: WinError::OutOfMemory,
            context: None,
        }
    }

    /// Fewer bytes were returned than expected.
    pub const fn unexpected_eof() -> Self {
        Self {
            kind: WinError::UnexpectedEof,
            context: None,
        }
    }

    /// Whether this error is `ERROR_NOT_READY` (no disc in drive).
    pub fn is_not_ready(&self) -> bool {
        matches!(self.kind, WinError::NotReady)
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            WinError::Io(error) => error.kind(),
            WinError::NotReady => io::Error::from_raw_os_error(ERROR_NOT_READY).kind(),
            WinError::InvalidData | WinError::Toc(_) => IoErrorKind::InvalidData,
            WinError::InvalidFilename => IoErrorKind::InvalidFilename,
            WinError::Unsupported => IoErrorKind::Unsupported,
            WinError::OutOfMemory => IoErrorKind::OutOfMemory,
            WinError::UnexpectedEof => IoErrorKind::UnexpectedEof,
            _ => todo!(),
        }
    }
}

impl Display for WinErrorOld {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for WinErrorOld {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<WinError> for WinErrorOld {
    fn from(kind: WinError) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<io::Error> for WinErrorOld {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<TocErrorOld> for WinErrorOld {
    fn from(error: TocErrorOld) -> Self {
        Self {
            kind: WinError::Toc(error),
            context: None,
        }
    }
}

impl From<WinErrorOld> for Error {
    fn from(error: WinErrorOld) -> Self {
        crate::error::ErrorKind::Win(error).into()
    }
}

impl From<WinErrorOld> for io::Error {
    fn from(error: WinErrorOld) -> Self {
        match error.kind {
            // return the original OS error directly, preserving raw_os_error (status quo: call
            // sites propagated the io::Error unchanged)
            WinError::Io(error) => error,
            WinError::NotReady => io::Error::from_raw_os_error(ERROR_NOT_READY),
            kind => io::Error::new(
                match &kind {
                    WinError::InvalidData | WinError::Toc(_) => IoErrorKind::InvalidData,
                    WinError::InvalidFilename => IoErrorKind::InvalidFilename,
                    WinError::Unsupported => IoErrorKind::Unsupported,
                    WinError::OutOfMemory => IoErrorKind::OutOfMemory,
                    WinError::UnexpectedEof => IoErrorKind::UnexpectedEof,
                    WinError::Io(_) | WinError::NotReady => unreachable!(),
                    _ => todo!(),
                },
                WinErrorOld {
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
pub enum WinError {
    /// An error related to a .cda file
    CdaError(CdaError), // Note to mistral: C-WORD-ORDER (delete this comment when read)
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
    Toc(TocErrorOld),
}

impl From<CdaError> for WinError {
    fn from(error: CdaError) -> Self {
        WinError::CdaError(error)
    }
}

#[derive(Debug)]
/// TODO - Half done example shape
pub enum CdaError {
    /// Track number outside valid range (1..=99)
    InvalidTrackNo(InvalidTrackNo),
}

impl From<InvalidTrackNo> for CdaError {
    fn from(error: InvalidTrackNo) -> Self {
        CdaError::InvalidTrackNo(error)
    }
}

impl Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WinError::Io(error) => Display::fmt(error, f),
            WinError::NotReady => f.write_str("device not ready (no disc in drive)"),
            WinError::InvalidData => f.write_str("invalid data"),
            WinError::InvalidFilename => f.write_str("invalid filename"),
            WinError::Unsupported => f.write_str("operation not supported"),
            WinError::OutOfMemory => f.write_str("buffer too large"),
            WinError::UnexpectedEof => f.write_str("unexpected end of data"),
            WinError::Toc(error) => Display::fmt(error, f),
            _ => todo!(),
        }
    }
}

impl StdError for WinError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            WinError::Io(error) => Some(error),
            WinError::Toc(error) => Some(error),
            WinError::NotReady
            | WinError::InvalidData
            | WinError::InvalidFilename
            | WinError::Unsupported
            | WinError::OutOfMemory
            | WinError::UnexpectedEof => None,
            _ => todo!(),
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
        assert_traits::<WinErrorOld>();
        assert_traits::<WinError>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(
            WinErrorOld::not_ready().to_string(),
            "device not ready (no disc in drive)"
        );
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = WinErrorOld::invalid_data().context("Missing or invalid RIFF header");
        assert_eq!(
            error.to_string(),
            "invalid data: Missing or invalid RIFF header"
        );
    }

    #[test]
    fn os_error_21_maps_to_not_ready() {
        let error = WinErrorOld::from(io::Error::from_raw_os_error(ERROR_NOT_READY));
        assert!(error.is_not_ready());
        assert!(matches!(error.kind(), WinError::NotReady));
        assert_eq!(
            io::Error::from(error).raw_os_error(),
            Some(ERROR_NOT_READY),
            "conversion back to io::Error preserves the raw OS error code"
        );
    }

    #[test]
    fn other_os_errors_keep_their_io_error() {
        let raw = io::Error::from_raw_os_error(5); // ERROR_ACCESS_DENIED
        let error = WinErrorOld::from(raw);
        assert!(matches!(error.kind(), WinError::Io(_)));
        let io_error = io::Error::from(error);
        assert_eq!(io_error.raw_os_error(), Some(5));
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(WinErrorOld::invalid_data()).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(WinErrorOld::invalid_filename()).kind(),
            IoErrorKind::InvalidFilename
        );
        assert_eq!(
            io::Error::from(WinErrorOld::unsupported()).kind(),
            IoErrorKind::Unsupported
        );
        assert_eq!(
            io::Error::from(WinErrorOld::out_of_memory()).kind(),
            IoErrorKind::OutOfMemory
        );
        assert_eq!(
            io::Error::from(WinErrorOld::unexpected_eof()).kind(),
            IoErrorKind::UnexpectedEof
        );
    }
}
