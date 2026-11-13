#![expect(missing_docs, reason = "TODO")]
//! Level-2 error for track encoding: the [`EncodingError`] enum.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`. Conversion to [`io::Error`] is explicit via
//! [`RedbookError::into_io_error`][crate::error::RedbookError::into_io_error], never via
//! `From<_> for io::Error`.
use std::{
    error::Error,
    fmt::{self, Display},
    io,
};

use flacenc::bitsink::ByteSink;

use crate::error::RedbookError;

/// Error returned by [`RippedTrack::to_flac`][crate::RippedTrack::to_flac].
///
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
pub enum EncodingError {
    /// The raw track data is not a whole number of samples; carries the raw data length in bytes.
    InvalidData(usize),
    FlacError(FlacError),
}

impl RedbookError for EncodingError {}

impl Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodingError::InvalidData(len) => write!(
                f,
                "invalid raw track data: {len} bytes is not a whole number of samples"
            ),
            EncodingError::FlacError(_) => write!(f, "FLAC encoding error"),
        }
    }
}

impl Error for EncodingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            EncodingError::FlacError(error) => Some(error),
            EncodingError::InvalidData(_) => None,
        }
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            EncodingError::InvalidData(_) => {
                request.provide_value(io::ErrorKind::InvalidData);
            }
            EncodingError::FlacError(_) => {
                // delegate
            }
        }
    }
}

impl From<FlacError> for EncodingError {
    fn from(error: FlacError) -> Self {
        Self::FlacError(error)
    }
}

impl From<flacenc::error::VerifyError> for EncodingError {
    fn from(error: flacenc::error::VerifyError) -> Self {
        FlacError::from(error).into()
    }
}

impl From<(flacenc::config::Encoder, flacenc::error::VerifyError)> for EncodingError {
    fn from(error: (flacenc::config::Encoder, flacenc::error::VerifyError)) -> Self {
        FlacError::from(error).into()
    }
}

impl From<flacenc::error::EncodeError> for EncodingError {
    fn from(error: flacenc::error::EncodeError) -> Self {
        FlacError::from(error).into()
    }
}

impl From<flacenc::error::SourceError> for EncodingError {
    fn from(error: flacenc::error::SourceError) -> Self {
        FlacError::from(error).into()
    }
}

impl From<flacenc::error::OutputError<ByteSink>> for EncodingError {
    fn from(error: flacenc::error::OutputError<ByteSink>) -> Self {
        FlacError::from(error).into()
    }
}

#[derive(Debug, Clone)]
pub enum FlacError {
    InvalidEncoder(flacenc::error::VerifyError),
    /// A [`flacenc`] encode failure. flacenc's `SourceErrorKind` is `!Send` (it can hold an
    /// `Rc<dyn Error>`), so only its message is retained (C-GOOD-ERR requires error types to be
    /// `Send + Sync`).
    ///
    /// TODO: #214 Revamp flacenc::SourceErrorKind handling when we improve typesafety around encoding
    InvalidData(String),
    OutputError(flacenc::error::OutputError<ByteSink>),
}

impl Error for FlacError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FlacError::InvalidEncoder(error) => Some(error),
            FlacError::OutputError(error) => Some(error),
            // documented exception to the tree rule: flacenc's SourceErrorKind is !Send, so only
            // its message is retained and there is no source to return
            FlacError::InvalidData(_) => None,
        }
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            FlacError::InvalidEncoder(_) => {
                request.provide_value(io::ErrorKind::Other);
            }
            FlacError::InvalidData(_) => {
                request.provide_value(io::ErrorKind::InvalidData);
            }
            FlacError::OutputError(output_error) => match &output_error {
                flacenc::error::OutputError::Range(_) => {
                    request.provide_value(io::ErrorKind::InvalidData);
                }
                flacenc::error::OutputError::Sink(infallible) => match *infallible {},
                _ => {
                    // non-exhaustive. Nothing is better than wrong...
                }
            },
        }
    }
}

impl Display for FlacError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlacError::InvalidEncoder(_) => write!(f, "invalid encoder configuration"),
            FlacError::InvalidData(message) => write!(f, "invalid source data: {message}"),
            FlacError::OutputError(_) => write!(f, "FLAC output error"),
        }
    }
}

impl From<flacenc::error::VerifyError> for FlacError {
    fn from(error: flacenc::error::VerifyError) -> Self {
        Self::InvalidEncoder(error)
    }
}

impl From<(flacenc::config::Encoder, flacenc::error::VerifyError)> for FlacError {
    fn from(error: (flacenc::config::Encoder, flacenc::error::VerifyError)) -> Self {
        Self::InvalidEncoder(error.1)
    }
}

impl From<flacenc::error::EncodeError> for FlacError {
    fn from(error: flacenc::error::EncodeError) -> Self {
        match error {
            flacenc::error::EncodeError::Source(source_error) => source_error.into(),
            flacenc::error::EncodeError::Config(verify_error) => verify_error.into(),
            _ => todo!(),
        }
    }
}

impl From<flacenc::error::SourceError> for FlacError {
    fn from(error: flacenc::error::SourceError) -> Self {
        Self::InvalidData(error.to_string())
    }
}

impl From<flacenc::error::OutputError<ByteSink>> for FlacError {
    fn from(error: flacenc::error::OutputError<ByteSink>) -> Self {
        Self::OutputError(error)
    }
}

impl RedbookError for FlacError {}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: Error + Send + Sync + 'static>() {}
        assert_traits::<EncodingError>();
        assert_traits::<FlacError>();
    }

    #[test]
    fn wrapped_child_errors_are_source_not_display() {
        let error = EncodingError::from(FlacError::InvalidData("boom".to_string()));
        assert_eq!(error.to_string(), "FLAC encoding error");
        let flac = Error::source(&error)
            .and_then(|s| s.downcast_ref::<FlacError>())
            .expect("source should be the FlacError");
        assert_eq!(flac.to_string(), "invalid source data: boom");
        // the documented !Send carve-out: no source below FlacError::InvalidData
        assert!(Error::source(flac).is_none());

        // data-carrying variant: rendered in Display, no source
        let error = EncodingError::InvalidData(2353);
        assert_eq!(
            error.to_string(),
            "invalid raw track data: 2353 bytes is not a whole number of samples"
        );
        assert!(Error::source(&error).is_none());
    }

    #[cfg(false)]
    #[test]
    fn display_renders_labels_and_data() {
        assert_eq!(
            EncodingError::InvalidData(2353).to_string(),
            "invalid raw track data: 2353 bytes is not a whole number of samples"
        );
        assert_eq!(
            EncodingError::Encode("boom".to_string()).to_string(),
            "flac encode failed: boom"
        );
    }

    #[cfg(false)]
    #[test]
    fn io_error_conversion_maps_to_invalid_data() {
        assert_eq!(
            io::Error::from(EncodingError::InvalidData(1)).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(EncodingError::Encode("x".to_string())).kind(),
            IoErrorKind::InvalidData
        );
    }

    #[cfg(false)]
    #[test]
    fn converts_into_root_error() {
        let error = Error::from(EncodingError::InvalidData(1));
        assert!(matches!(error, Error::Track(EncodingError::InvalidData(1))));
    }
}
