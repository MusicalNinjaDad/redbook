//! Level-2 error for track encoding: the [`TrackError`] enum.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, and `From<TrackError> for io::Error` mapping to
//! [`io::ErrorKind::InvalidData`] (encoding failures are data failures).
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use flacenc::{
    bitsink::ByteSink,
    error::{EncodeError, OutputError},
};

use crate::error::Error;

/// Error returned by [`RippedTrack::to_flac`][crate::RippedTrack::to_flac].
///
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
pub enum TrackError {
    /// The raw track data is not a whole number of samples; carries the raw data length in bytes.
    InvalidData(usize),
    /// A [`flacenc`] encode failure. flacenc's `EncodeError` is `!Send` (it can hold an
    /// `Rc<dyn Error>`), so only its message is retained (C-GOOD-ERR requires error types to be
    /// `Send + Sync`).
    Encode(String),
    /// A wrapped [`flacenc`] output error.
    Output(OutputError<ByteSink>),
}

impl TrackError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`].
    pub fn io_kind(&self) -> IoErrorKind {
        IoErrorKind::InvalidData
    }
}

impl Display for TrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackError::InvalidData(len) => write!(
                f,
                "invalid raw track data: {len} bytes is not a whole number of samples"
            ),
            TrackError::Encode(message) => write!(f, "flac encode failed: {message}"),
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            TrackError::Output(_) => f.write_str("flac output error"),
        }
    }
}

impl StdError for TrackError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TrackError::Output(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            TrackError::InvalidData(_) | TrackError::Encode(_) => None,
        }
    }
}

impl From<EncodeError> for TrackError {
    fn from(error: EncodeError) -> Self {
        // flacenc's EncodeError is !Send (it can hold an Rc<dyn Error>), so it cannot be wrapped
        // in a Send + Sync error (C-GOOD-ERR); its message is retained instead.
        TrackError::Encode(error.to_string())
    }
}

impl From<OutputError<ByteSink>> for TrackError {
    fn from(error: OutputError<ByteSink>) -> Self {
        TrackError::Output(error)
    }
}

impl From<TrackError> for Error {
    fn from(error: TrackError) -> Self {
        Error::Track(error)
    }
}

impl From<TrackError> for io::Error {
    fn from(error: TrackError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Interop downgrade to [`flacenc`]'s error type: everything maps to its closest semantic, a
/// config-style [`flacenc::error::VerifyError`] (flacenc's own `EncodeError` is !Send and cannot
/// be reconstructed losslessly from the retained message).
impl From<TrackError> for EncodeError {
    fn from(error: TrackError) -> Self {
        match error {
            TrackError::Encode(message) => {
                EncodeError::Config(flacenc::error::VerifyError::new("track", &message))
            }
            TrackError::InvalidData(_) | TrackError::Output(_) => EncodeError::Config(
                flacenc::error::VerifyError::new("track", "invalid raw track data"),
            ),
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
        assert_traits::<TrackError>();
    }

    #[test]
    fn display_renders_labels_and_data() {
        assert_eq!(
            TrackError::InvalidData(2353).to_string(),
            "invalid raw track data: 2353 bytes is not a whole number of samples"
        );
        assert_eq!(
            TrackError::Encode("boom".to_string()).to_string(),
            "flac encode failed: boom"
        );
    }

    #[test]
    fn io_error_conversion_maps_to_invalid_data() {
        assert_eq!(
            io::Error::from(TrackError::InvalidData(1)).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(TrackError::Encode("x".to_string())).kind(),
            IoErrorKind::InvalidData
        );
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(TrackError::InvalidData(1));
        assert!(matches!(error, Error::Track(TrackError::InvalidData(1))));
    }
}
