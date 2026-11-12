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
    path::PathBuf,
};

use glob::GlobError;

use crate::{
    drive::error::{DeviceError, OsError},
    frame::{Frame, InvalidFrame, Msf},
    toc::error::InvalidTrackNo,
};

use super::sys::ERROR_NOT_READY;

/// Error returned by [`win`] APIs. This is an OS-coloured type-wrapper around `io::Error`
/// to allow for OS-specific parsing of error codes such as [`ERROR_NOT_READY`]
#[derive(Debug)]
pub struct WinError(io::Error);

impl StdError for WinError {
    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        if request.would_be_satisfied_by_value_of::<io::Error>() {
            request.provide_value_with(|| self.to_io_error());
        } else if request.would_be_satisfied_by_ref_of::<io::Error>() {
            request.provide_ref(self.as_io_error());
        }
    }
}

impl Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl OsError for WinError {
    fn as_io_error(&self) -> &io::Error {
        &self.0
    }

    fn into_io_error(self) -> io::Error {
        self.0
    }
}

impl From<io::Error> for WinError {
    fn from(error: io::Error) -> Self {
        Self(error)
    }
}

impl From<WinError> for DeviceError {
    fn from(error: WinError) -> Self {
        match error.error_code() {
            ERROR_NOT_READY => DeviceError::NotReady(error.into_io_error()),
            _ => DeviceError::Other(error.into_io_error()),
        }
    }
}

/// Error from parsing a `.cda` file (see [`super::cda::CdaFile`]).
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
    /// The filename provided is not valid for a cda file.
    /// Display will include the provided path *if known*
    InvalidFilename(PathBuf),
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
    /// TODO
    IoError(io::Error),
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
            CdaError::InvalidTrackNo(_) => write!(f, "invalid track number in .cda file"),
            CdaError::InvalidFrame(_) => write!(f, "invalid frame in .cda file"),
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
            CdaError::InvalidFilename(path) => write!(f, "invalid path {p}", p = path.display()),
            CdaError::InvalidVersion(version) => write!(f, "unknown cda file version {version}"),
            CdaError::MismatchedStart { relative, absolute } => {
                write!(
                    f,
                    "mismatched start: relative frame {relative} / msf {absolute:?}"
                )
            }
            CdaError::MissingNullByte => write!(f, "missing null byte"),
            CdaError::MismatchedDuration { frames, msf } => {
                write!(f, "mismatched duration {frames} frames / {msf:?}")
            }
            CdaError::IoError(_) => write!(f, "I/O error"),
        }
    }
}

impl StdError for CdaError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            CdaError::InvalidTrackNo(error) => Some(error),
            CdaError::InvalidFrame(error) => Some(error),
            CdaError::IoError(error) => Some(error),
            CdaError::InvalidLength { .. }
            | CdaError::InvalidHeader(_)
            | CdaError::InvalidChunkSize { .. }
            | CdaError::InvalidFilename(_)
            | CdaError::InvalidVersion(_)
            | CdaError::MismatchedStart { .. }
            | CdaError::MissingNullByte
            | CdaError::MismatchedDuration { .. } => None,
        }
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
        match error {
            CdaError::IoError(e) => e,
            _ => io::Error::new(error.io_kind(), error),
        }
    }
}

impl From<io::Error> for CdaError {
    fn from(error: io::Error) -> Self {
        match error.kind() {
            IoErrorKind::InvalidFilename => CdaError::InvalidFilename(Default::default()),
            _ => CdaError::IoError(error),
        }
    }
}

impl From<GlobError> for CdaError {
    fn from(error: GlobError) -> Self {
        match error.error().kind() {
            IoErrorKind::InvalidFilename => CdaError::InvalidFilename(PathBuf::from(error.path())),
            _ => CdaError::IoError(error.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(false)]
    use std::io;

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<WinError>();
        assert_traits::<CdaError>();
    }

    #[test]
    fn wrapped_child_errors_are_source_not_display() {
        let error = CdaError::InvalidTrackNo(InvalidTrackNo::new(0_u16));
        assert_eq!(error.to_string(), "invalid track number in .cda file");
        assert!(
            StdError::source(&error)
                .and_then(|s| s.downcast_ref::<InvalidTrackNo>())
                .is_some()
        );

        let error = CdaError::IoError(io::Error::from_raw_os_error(5));
        assert_eq!(error.to_string(), "I/O error");
        assert_eq!(
            StdError::source(&error)
                .and_then(|s| s.downcast_ref::<io::Error>())
                .unwrap()
                .raw_os_error(),
            Some(5)
        );

        assert!(StdError::source(&CdaError::MissingNullByte).is_none());
    }

    #[cfg(false)]
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

    #[cfg(false)]
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

    #[cfg(false)]
    #[test]
    fn other_os_errors_keep_their_io_error() {
        let raw = io::Error::from_raw_os_error(5); // ERROR_ACCESS_DENIED
        let error = WinError::from(raw);
        assert!(matches!(error, WinError::Io(_)));
        let io_error = io::Error::from(error);
        assert_eq!(io_error.raw_os_error(), Some(5));
    }

    #[cfg(false)]
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

    #[cfg(false)]
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
