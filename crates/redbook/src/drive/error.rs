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
    io::{self, ErrorKind as IoErrorKind, RawOsError},
};

use thread_safely::Context;

use crate::{
    RipProgress,
    error::Error,
    frame::{InvalidByteSize, InvalidFrame},
    toc::{TocError, TrackNo},
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
    /// The track has zero duration.
    ZeroLengthTrack,
    /// Sending a progress update on the [`Context`][thread_safely::Context] channel failed.
    Progress(RipProgress),
    /// TODO
    Device(DeviceError),
    /// A thread cancellation request was received
    ThreadCancellation,
    /// Data read or computed from the inserted disc is invalid
    InvalidData(InvalidData),
}

impl StdError for InvalidData {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            InvalidData::Toc(toc_error) => Some(toc_error),
            InvalidData::InvalidFrame(invalid_frame) => Some(invalid_frame),
        }
    }
}

impl Display for InvalidData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidData::Toc(_) => write!(f, "bad TOC read from disc"),
            InvalidData::InvalidFrame(_) => write!(
                f,
                "attempt to reference an invalid frame while reading disc"
            ),
        }
    }
}

impl DriveError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match self {
            DriveError::NotFound(_) => IoErrorKind::NotFound,
            DriveError::ZeroLengthTrack => IoErrorKind::UnexpectedEof,
            DriveError::Progress(_) => IoErrorKind::Other,
            DriveError::Device(_) => todo!(),
            DriveError::ThreadCancellation => todo!(),
            DriveError::InvalidData(_invalid_data) => todo!(),
        }
    }
}

impl Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveError::NotFound(track_number) => {
                write!(f, "track {} not found on disc", track_number.cast::<u8>())
            }
            DriveError::ZeroLengthTrack => f.write_str("unexpected end of data"),
            DriveError::Progress(RipProgress { track_number, .. }) => {
                write!(f, "failed to update progress for {track_number}")
            }
            DriveError::Device(_) => write!(f, "device error"),
            DriveError::ThreadCancellation => write!(f, "thread cancelled"),
            DriveError::InvalidData(invalid_data) => Display::fmt(invalid_data, f),
        }
    }
}

impl StdError for DriveError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DriveError::Device(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            DriveError::NotFound(_)
            | DriveError::ZeroLengthTrack
            | DriveError::Progress(_)
            | DriveError::ThreadCancellation
            | DriveError::InvalidData(_) => None,
        }
    }
}

impl From<TocError> for DriveError {
    fn from(error: TocError) -> Self {
        InvalidData::from(error).into()
    }
}

impl From<InvalidFrame> for DriveError {
    fn from(error: InvalidFrame) -> Self {
        InvalidData::from(error).into()
    }
}

impl From<InvalidData> for DriveError {
    fn from(error: InvalidData) -> Self {
        DriveError::InvalidData(error)
    }
}

impl From<crossbeam::channel::SendError<RipProgress>> for DriveError {
    fn from(error: crossbeam::channel::SendError<RipProgress>) -> Self {
        Self::Progress(error.0)
    }
}

impl From<DeviceError> for DriveError {
    fn from(error: DeviceError) -> Self {
        DriveError::Device(error)
    }
}

impl From<Context<RipProgress>> for DriveError {
    fn from(_: Context<RipProgress>) -> Self {
        DriveError::ThreadCancellation
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

#[derive(Debug)]
/// TODO
pub enum InvalidData {
    /// An error while processing the TOC obtained from the drive
    Toc(TocError),
    /// adsf
    InvalidFrame(InvalidFrame),
}

impl From<TocError> for InvalidData {
    fn from(error: TocError) -> Self {
        InvalidData::Toc(error)
    }
}

impl From<InvalidFrame> for InvalidData {
    fn from(error: InvalidFrame) -> Self {
        InvalidData::InvalidFrame(error)
    }
}

/// Errors returned by [`trait Device`][crate::Device]
///
/// TODO: Keep the genuine io::Errors in all relevant variants
#[derive(Debug)]
pub enum DeviceError {
    /// The provided/calculated buffer size is unsuitable for the purpose
    /// or too large for this architecture
    InvalidBufferSize(i128),
    /// The device is not ready - usually because no disc is inserted
    NotReady,
    /// Less data was read than expected
    UnexpectedEndOfRead {
        /// The number of bytes expected.
        expected: u32,
        /// The number of bytes actually received.
        received: u32,
    },
    /// The device does not support CD-ROM operations
    UnsupportedDevice,
    /// Some other io::Error occurred
    Other(io::Error),
}

impl DeviceError {
    /// TODO
    pub fn io_kind(&self) -> io::ErrorKind {
        todo!()
    }
}

impl StdError for DeviceError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        use DeviceError::*;

        match self {
            Other(e) => Some(e),
            InvalidBufferSize(_) | NotReady | UnexpectedEndOfRead { .. } | UnsupportedDevice => {
                None
            }
        }
    }
}

impl Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::InvalidBufferSize(_) => todo!(),
            DeviceError::NotReady => write!(f, "device not ready, is a disc inserted?"),
            DeviceError::UnexpectedEndOfRead { expected, received } => {
                write!(f, "expected {expected} bytes, received {received}")
            }
            DeviceError::UnsupportedDevice => write!(f, "device does not support CD operations"),
            DeviceError::Other(error) => Display::fmt(error, f),
        }
    }
}

impl From<InvalidByteSize> for DeviceError {
    fn from(error: InvalidByteSize) -> Self {
        Self::InvalidBufferSize(error.invalid_value())
    }
}

/// Trait for Errors which provide OS-colouring around an io::Error
///
/// # Note for implementors
///
/// Implementations must guarantee that construction is only possible from a genuine OS error - either
/// via [`io::Error::last_os_error`] or [`io::Error::from_raw_os_error`], otherwise the provided
/// function [`error_code`][OsError::error_code] will panic.
///
/// Most implementations will be a simple new-type wrapper to provide colouring
/// TODO: include WinError as full example
pub trait OsError:
    StdError + Send + Sync + 'static // C-GOOD-ERR
    + From<io::Error>
    + Into<DeviceError>
{
    /// Generate an OsError from the last os error this thread produced.
    /// 
    /// See [`io::Error::last_os_error`], which the provided default implementation delegates to,
    /// for more details.
    /// 
    /// This function is provided to allow for custom implementations - as long as the resulting
    /// error can infallibly produce an `io::Error` with a valid `RawOsError` code
    fn get_from_os() -> Self {
        io::Error::last_os_error().into()
    }

    /// Obtain a reference to the underlying io::Error
    fn as_io_error(&self) -> &io::Error;

    /// Consume the OsError wrapper and return the wrapped io::Error
    fn into_io_error(self) -> io::Error;

    /// Generate a new, owned, io::Error based on the underlying error code
    /// 
    /// This is a workaround the fact that io::Error is not clone
    fn to_io_error(&self) -> io::Error {
        io::Error::from_raw_os_error(self.error_code())
    }

    /// Obtain the raw os error code related to this error.
    fn error_code(&self) -> RawOsError {
        self.as_io_error().raw_os_error().expect("Construction only possible from a raw os error")
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<DriveError>();
    }

    #[cfg(false)]
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

    #[cfg(false)]
    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(DriveError::NotFound(TrackNo::strict_new(1))).kind(),
            IoErrorKind::NotFound
        );
        assert_eq!(
            io::Error::from(DriveError::ZeroLengthTrack).kind(),
            IoErrorKind::UnexpectedEof
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

    #[cfg(false)]
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
        let error = Error::from(DriveError::ZeroLengthTrack);
        assert!(matches!(error, Error::Drive(DriveError::ZeroLengthTrack)));
    }
}
