//! Level-2 error for the [`win`] module (Windows hardware access): the [`WinError`] enum, plus
//! [`CdaError`] for `.cda` file parsing.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]. In addition, OS errors get
//! dedicated handling: a wrapped [`io::Error`] from the OS is returned directly from
//! `From<WinError> for io::Error` (preserving `raw_os_error`), and `ERROR_NOT_READY` (21, no disc
//! in drive) is surfaced as the dedicated [`WinError::NotReady`] variant.
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use crate::{
    error::Error,
    frame::{Frame, InvalidFrame, Msf},
    toc::{TocError, error::InvalidTrackNo},
};

/// `ERROR_NOT_READY`: the device is not ready, e.g. no disc in the drive.
pub const ERROR_NOT_READY: i32 = 21;

/// Error returned by [`win`] APIs: OS/hardware failures and `.cda` file parsing failures.
///
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
pub enum WinError {
    /// An error parsing a `.cda` file.
    CdaError(CdaError),
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
    UnexpectedEof {
        /// The number of bytes expected.
        expected: u32,
        /// The number of bytes actually received.
        received: u32,
    },
    /// A wrapped [`toc`][crate::toc] error, e.g. from parsing the raw TOC.
    Toc(TocError),
}

impl WinError {
    /// Whether this error is `ERROR_NOT_READY` (no disc in drive).
    pub fn is_not_ready(&self) -> bool {
        matches!(self, WinError::NotReady)
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match self {
            WinError::CdaError(error) => error.io_kind(),
            WinError::Io(error) => error.kind(),
            WinError::NotReady => io::Error::from_raw_os_error(ERROR_NOT_READY).kind(),
            WinError::InvalidData | WinError::Toc(_) => IoErrorKind::InvalidData,
            WinError::InvalidFilename => IoErrorKind::InvalidFilename,
            WinError::Unsupported => IoErrorKind::Unsupported,
            WinError::OutOfMemory => IoErrorKind::OutOfMemory,
            WinError::UnexpectedEof { .. } => IoErrorKind::UnexpectedEof,
        }
    }
}

impl Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            WinError::CdaError(_) => f.write_str("invalid .cda file"),
            WinError::Io(_) => f.write_str("I/O error"),
            WinError::Toc(_) => f.write_str("TOC error"),
            WinError::NotReady => f.write_str("device not ready (no disc in drive)"),
            WinError::InvalidData => f.write_str("invalid data"),
            WinError::InvalidFilename => f.write_str("invalid filename"),
            WinError::Unsupported => f.write_str("operation not supported"),
            WinError::OutOfMemory => f.write_str("buffer too large"),
            WinError::UnexpectedEof { expected, received } => {
                write!(f, "expected {expected} bytes, received {received}")
            }
        }
    }
}

impl StdError for WinError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            WinError::CdaError(error) => Some(error),
            WinError::Io(error) => Some(error),
            WinError::Toc(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            WinError::NotReady
            | WinError::InvalidData
            | WinError::InvalidFilename
            | WinError::Unsupported
            | WinError::OutOfMemory
            | WinError::UnexpectedEof { .. } => None,
        }
    }
}

impl From<io::Error> for WinError {
    fn from(error: io::Error) -> Self {
        if error.raw_os_error() == Some(ERROR_NOT_READY) {
            WinError::NotReady
        } else {
            WinError::Io(error)
        }
    }
}

impl From<CdaError> for WinError {
    fn from(error: CdaError) -> Self {
        WinError::CdaError(error)
    }
}

impl From<TocError> for WinError {
    fn from(error: TocError) -> Self {
        WinError::Toc(error)
    }
}

impl From<WinError> for Error {
    fn from(error: WinError) -> Self {
        Error::Win(error)
    }
}

impl From<WinError> for io::Error {
    fn from(error: WinError) -> Self {
        match error {
            // return the original OS error directly, preserving raw_os_error (status quo: call
            // sites propagated the io::Error unchanged)
            WinError::Io(error) => error,
            WinError::NotReady => io::Error::from_raw_os_error(ERROR_NOT_READY),
            other => io::Error::new(other.io_kind(), other),
        }
    }
}

/// Error from parsing a `.cda` file (see [`super::cda::CdaFile`]).
///
/// Variants carry the data which led to the error; leaf payloads are rendered by [`Display`]
/// and return `None` from `source()` (issue #198).
#[derive(Debug)]
pub enum CdaError {
    /// The track number is outside the valid range 1..=99.
    InvalidTrackNo(InvalidTrackNo),
    /// A frame value (duration or relative start) is above [`Frame::MAX`].
    InvalidFrame(InvalidFrame),
    /// The file length is wrong.
    InvalidLength {
        /// The expected length in bytes ([`super::cda::CDA_LEN`]).
        expected: usize,
        /// The actual length in bytes.
        actual: usize,
    },
    /// A magic header (RIFF / CDDA / fmt ) is missing or invalid; carries the bytes read.
    InvalidHeader([u8; 4]),
    /// A chunk size field has an unexpected value.
    InvalidChunkSize {
        /// The expected chunk size.
        expected: u32,
        /// The actual chunk size read.
        actual: u32,
    },
    /// The format version is unknown.
    InvalidVersion(u16),
    /// The relative frame offset and the absolute MSF disagree.
    MismatchedStart {
        /// The relative frame offset as stored at 0x1c.
        relative: u32,
        /// The absolute MSF as stored at 0x24.
        absolute: Msf,
    },
    /// An expected null byte is missing.
    MissingNullByte,
    /// The duration in frames and the duration MSF disagree.
    MismatchedDuration {
        /// The duration as a frame count.
        frames: Frame,
        /// The duration as MSF.
        msf: Msf,
    },
}

impl CdaError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`]: `.cda` parsing
    /// failures are data failures.
    pub fn io_kind(&self) -> IoErrorKind {
        IoErrorKind::InvalidData
    }
}

impl Display for CdaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CdaError::InvalidTrackNo(error) => Display::fmt(error, f),
            CdaError::InvalidFrame(error) => Display::fmt(error, f),
            CdaError::InvalidLength { expected, actual } => {
                write!(
                    f,
                    "invalid CDA file: expected {expected} bytes, got {actual}"
                )
            }
            CdaError::InvalidHeader(magic) => {
                write!(f, "missing or invalid CDA header: {magic:?}")
            }
            CdaError::InvalidChunkSize { expected, actual } => {
                write!(f, "invalid chunk size: expected {expected}, got {actual}")
            }
            CdaError::InvalidVersion(version) => write!(f, "unknown cda file version {version}"),
            CdaError::MismatchedStart { relative, absolute } => {
                write!(
                    f,
                    "mismatched start: relative frame {relative} / msf {absolute:?}"
                )
            }
            CdaError::MissingNullByte => f.write_str("missing null byte"),
            CdaError::MismatchedDuration { frames, msf } => {
                write!(f, "mismatched duration {frames} frames / {msf:?}")
            }
        }
    }
}

impl StdError for CdaError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        // all variants carry data which is rendered in Display (issue #198)
        None
    }
}

impl From<InvalidTrackNo> for CdaError {
    fn from(error: InvalidTrackNo) -> Self {
        CdaError::InvalidTrackNo(error)
    }
}

impl From<InvalidFrame> for CdaError {
    fn from(error: InvalidFrame) -> Self {
        CdaError::InvalidFrame(error)
    }
}

impl From<CdaError> for io::Error {
    fn from(error: CdaError) -> Self {
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
        assert_traits::<WinError>();
        assert_traits::<CdaError>();
    }

    #[test]
    fn display_renders_labels_and_data() {
        assert_eq!(
            WinError::NotReady.to_string(),
            "device not ready (no disc in drive)"
        );
        assert_eq!(
            WinError::UnexpectedEof {
                expected: 44,
                received: 12
            }
            .to_string(),
            "expected 44 bytes, received 12"
        );
        assert_eq!(
            CdaError::InvalidTrackNo(InvalidTrackNo::new(0_u16)).to_string(),
            "invalid track number: 0x0000"
        );
    }

    #[test]
    fn os_error_21_maps_to_not_ready() {
        let error = WinError::from(io::Error::from_raw_os_error(ERROR_NOT_READY));
        assert!(error.is_not_ready());
        assert!(matches!(error, WinError::NotReady));
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
        assert!(matches!(error, WinError::Io(_)));
        let io_error = io::Error::from(error);
        assert_eq!(io_error.raw_os_error(), Some(5));
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(WinError::InvalidData).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(WinError::InvalidFilename).kind(),
            IoErrorKind::InvalidFilename
        );
        assert_eq!(
            io::Error::from(WinError::Unsupported).kind(),
            IoErrorKind::Unsupported
        );
        assert_eq!(
            io::Error::from(WinError::OutOfMemory).kind(),
            IoErrorKind::OutOfMemory
        );
        assert_eq!(
            io::Error::from(WinError::UnexpectedEof {
                expected: 1,
                received: 0
            })
            .kind(),
            IoErrorKind::UnexpectedEof
        );
        assert_eq!(
            io::Error::from(WinError::CdaError(CdaError::MissingNullByte)).kind(),
            IoErrorKind::InvalidData
        );
    }

    #[test]
    fn source_returns_wrapped_errors_only() {
        let error = WinError::from(io::Error::from_raw_os_error(5));
        assert!(
            StdError::source(&error)
                .and_then(|s| s.downcast_ref::<io::Error>())
                .is_some()
        );
        assert!(StdError::source(&WinError::NotReady).is_none());
        let error = WinError::from(TocError::NotFound);
        assert!(
            StdError::source(&error)
                .and_then(|s| s.downcast_ref::<TocError>())
                .is_some()
        );
    }
}
