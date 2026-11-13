//! Level-2 error for the [`win`] module (Windows hardware access): the [`WinError`] struct, plus
//! [`CdaError`] for `.cda` file parsing.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]. In addition, OS errors get
//! dedicated handling via [`OsError`]: the wrapped [`io::Error`] from the OS is available
//! directly (preserving `raw_os_error`), and `ERROR_NOT_READY` (21, no disc in drive) is
//! surfaced as [`DeviceError::NotReady`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
    path::PathBuf,
};

use glob::GlobError;

use crate::{
    drive::error::{DeviceError, OsError},
    error::RedbookError,
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

impl RedbookError for CdaError {}

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

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            CdaError::InvalidTrackNo(_)
            | CdaError::InvalidFrame(_)
            | CdaError::InvalidLength { .. }
            | CdaError::InvalidHeader(_)
            | CdaError::InvalidChunkSize { .. }
            | CdaError::InvalidVersion(_)
            | CdaError::MismatchedStart { .. }
            | CdaError::MissingNullByte
            | CdaError::MismatchedDuration { .. } => {
                request.provide_value(io::ErrorKind::InvalidData);
            }
            CdaError::InvalidFilename(_) => {
                request.provide_value(io::ErrorKind::InvalidFilename);
            }
            CdaError::IoError(io_error) => {
                request.provide_value(io_error.kind());
            }
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
