#![expect(missing_docs, reason = "TODO")]
//! Errors for the Drive domain: [DriveError], [DeviceError]
use std::{
    error::Error,
    fmt::{self, Display},
    io::{self, RawOsError},
};

use thread_safely::Context;

use crate::{
    RipProgress,
    error::RedbookError,
    frame::{InvalidByteSize, InvalidFrame},
    toc::{
        TocError, TrackNo,
        error::{InvalidOrder, InvalidTrackNo},
    },
    win::WinError,
};

/// Error returned by [`Drive`][crate::Drive] APIs. Covers a possible wrapped [DeviceError] plus
/// higher-level concerns.
#[derive(Debug)]
pub enum DriveError {
    DeviceError(DeviceError),
    /// Attempted to read from an invalid location
    InvalidFrame(InvalidFrame),
    /// The requested track was not found on the disc; carries the track number
    NotFound(TrackNo),
    /// No CD drives were found on the system; [`CdDrives::all`][crate::drive::CdDrives::all]
    /// yielded no drives.
    NoDrivesFound,
    /// The track has zero duration
    ZeroLengthTrack,
    /// Sending a progress update on the [`Context`][thread_safely::Context] channel failed.
    Progress(RipProgress),
    /// A thread cancellation request was received via the [`Context`]
    ThreadCancellation,
}

impl RedbookError for DriveError {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        match self {
            DriveError::DeviceError(_) => io::ErrorKind::Other,
            DriveError::InvalidFrame(_) => io::ErrorKind::InvalidData,
            DriveError::NotFound(_) => io::ErrorKind::NotFound,
            DriveError::NoDrivesFound => io::ErrorKind::NotFound,
            DriveError::ZeroLengthTrack => io::ErrorKind::UnexpectedEof,
            DriveError::Progress(_) => io::ErrorKind::Other,
            DriveError::ThreadCancellation => io::ErrorKind::Interrupted,
        }
    }
}

impl Error for DriveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            DriveError::DeviceError(error) => Some(error),
            DriveError::InvalidFrame(error) => Some(error),
            DriveError::NotFound(_)
            | DriveError::NoDrivesFound
            | DriveError::ZeroLengthTrack
            | DriveError::Progress(_)
            | DriveError::ThreadCancellation => None,
        }
    }
}

impl Display for DriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveError::NotFound(track_number) => {
                write!(f, "track {} not found on disc", track_number.cast::<u8>())
            }
            DriveError::NoDrivesFound => write!(f, "no drives found"),
            DriveError::ZeroLengthTrack => write!(f, "unexpected end of data"),
            DriveError::Progress(RipProgress { track_number, .. }) => {
                write!(f, "failed to update progress for {track_number}")
            }
            DriveError::DeviceError(_) => write!(f, "device error"),
            DriveError::InvalidFrame(_) => write!(f, "invalid frame"),
            DriveError::ThreadCancellation => write!(f, "thread cancelled"),
        }
    }
}

impl From<DeviceError> for DriveError {
    fn from(error: DeviceError) -> Self {
        Self::DeviceError(error)
    }
}

impl From<InvalidBufferSize> for DriveError {
    fn from(error: InvalidBufferSize) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<InvalidByteSize> for DriveError {
    fn from(error: InvalidByteSize) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<WinError> for DriveError {
    fn from(error: WinError) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<TocError> for DriveError {
    fn from(error: TocError) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<cdtoc::TocError> for DriveError {
    fn from(error: cdtoc::TocError) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<InvalidTrackNo> for DriveError {
    fn from(error: InvalidTrackNo) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<InvalidOrder> for DriveError {
    fn from(error: InvalidOrder) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<InvalidFrame> for DriveError {
    fn from(error: InvalidFrame) -> Self {
        Self::InvalidFrame(error)
    }
}

impl From<crossbeam::channel::SendError<RipProgress>> for DriveError {
    fn from(error: crossbeam::channel::SendError<RipProgress>) -> Self {
        Self::Progress(error.0)
    }
}

impl From<Context<RipProgress>> for DriveError {
    fn from(_: Context<RipProgress>) -> Self {
        Self::ThreadCancellation
    }
}

/// Errors returned by [`trait Device`][crate::Device]
#[derive(Debug)]
pub enum DeviceError {
    /// The provided/calculated buffer size is unsuitable for the purpose
    InvalidBufferSize(InvalidBufferSize),
    /// The device is not ready - usually because no disc is inserted
    NotReady(io::Error),
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
    TocError(TocError),
}

impl RedbookError for DeviceError {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        match self {
            DeviceError::InvalidBufferSize(_) => io::ErrorKind::InvalidData,
            DeviceError::NotReady(_) | DeviceError::Other(_) => io::ErrorKind::Other,
            DeviceError::UnexpectedEndOfRead { .. } => io::ErrorKind::UnexpectedEof,
            DeviceError::UnsupportedDevice => io::ErrorKind::Unsupported,
            DeviceError::TocError(_) => io::ErrorKind::InvalidData,
        }
    }
}

impl Error for DeviceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        use DeviceError::*;

        match self {
            Other(e) => Some(e),
            NotReady(e) => Some(e),
            TocError(e) => Some(e),
            InvalidBufferSize(e) => Some(e),
            UnexpectedEndOfRead { .. } | UnsupportedDevice => None,
        }
    }
}

impl Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceError::InvalidBufferSize(_) => write!(f, "invalid buffer size"),
            DeviceError::NotReady(_) => write!(f, "device not ready"),
            DeviceError::TocError(_) => write!(f, "TOC error"),
            DeviceError::Other(_) => write!(f, "I/O error"),
            DeviceError::UnexpectedEndOfRead { expected, received } => {
                write!(f, "expected {expected} bytes, received {received}")
            }
            DeviceError::UnsupportedDevice => write!(f, "device does not support CD operations"),
        }
    }
}

impl From<InvalidBufferSize> for DeviceError {
    fn from(error: InvalidBufferSize) -> Self {
        Self::InvalidBufferSize(error)
    }
}

impl From<InvalidByteSize> for DeviceError {
    fn from(error: InvalidByteSize) -> Self {
        InvalidBufferSize::from(error).into()
    }
}

impl From<TocError> for DeviceError {
    fn from(error: TocError) -> Self {
        Self::TocError(error)
    }
}

impl From<cdtoc::TocError> for DeviceError {
    fn from(error: cdtoc::TocError) -> Self {
        TocError::from(error).into()
    }
}

impl From<InvalidTrackNo> for DeviceError {
    fn from(error: InvalidTrackNo) -> Self {
        TocError::from(error).into()
    }
}

impl From<InvalidOrder> for DeviceError {
    fn from(error: InvalidOrder) -> Self {
        TocError::from(error).into()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InvalidBufferSize {
    TooSmall { provided: usize, required: u128 },
    InvalidByteSize(InvalidByteSize),
}

impl Error for InvalidBufferSize {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            InvalidBufferSize::InvalidByteSize(error) => Some(error),
            InvalidBufferSize::TooSmall { .. } => None,
        }
    }
}

impl Display for InvalidBufferSize {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InvalidBufferSize::InvalidByteSize(_) => write!(f, "invalid byte size"),
            InvalidBufferSize::TooSmall { provided, required } => {
                write!(
                    f,
                    "buffer size {provided} is too small; {required} required"
                )
            }
        }
    }
}

impl RedbookError for InvalidBufferSize {
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        io::ErrorKind::InvalidData
    }
}

impl From<InvalidByteSize> for InvalidBufferSize {
    fn from(error: InvalidByteSize) -> Self {
        Self::InvalidByteSize(error)
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
///
/// # Boundary
///
/// `OsError` and [`RedbookError`] are mutually exclusive: an `OsError` is only produced by
/// low-level [`Device`][crate::Device] implementations and is consumed when converting into a
/// [`RedbookError`] (e.g. `From<WinError> for DeviceError`); it must not leak past the boundary
/// of a `Device` implementation. The trait remains `pub` so that third-party `Device`
/// implementations can use it.
pub trait OsError:
    Error + Send + Sync + 'static // C-GOOD-ERR
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
        fn assert_traits<T: Error + Send + Sync + 'static>() {}
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
        let source = Error::source(&error).expect("wrapped win error should be the source");
        let win = source
            .downcast_ref::<WinError>()
            .expect("source should be the WinError directly");
        assert!(win.is_not_ready());
    }

    #[test]
    fn converts_into_root_error() {
        let error = crate::Error::from(DriveError::ZeroLengthTrack);
        assert!(matches!(
            error,
            crate::Error::DriveError(DriveError::ZeroLengthTrack)
        ));
    }

    #[test]
    fn wrapped_child_errors_are_source_not_display() {
        let error = DeviceError::Other(io::Error::from_raw_os_error(5));
        assert_eq!(error.to_string(), "I/O error");
        let source = Error::source(&error).expect("wrapped io::Error should be the source");
        assert_eq!(
            source.downcast_ref::<io::Error>().unwrap().raw_os_error(),
            Some(5)
        );

        let error = DriveError::from(InvalidFrame::new(0x1_0000_0000u64));
        assert_eq!(error.to_string(), "invalid frame");
        assert!(
            Error::source(&error)
                .and_then(|s| s.downcast_ref::<InvalidFrame>())
                .is_some()
        );

        let buffer = InvalidBufferSize::TooSmall {
            provided: 1,
            required: 2,
        };
        let error = DeviceError::from(buffer);
        assert_eq!(error.to_string(), "invalid buffer size");
        assert!(
            Error::source(&error)
                .and_then(|s| s.downcast_ref::<InvalidBufferSize>())
                .is_some()
        );
        assert!(Error::source(&buffer).is_none());
    }
}
