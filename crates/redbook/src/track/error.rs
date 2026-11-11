#![expect(missing_docs, reason = "TODO")]
//! Level-2 error for track encoding: the [`TrackError`] enum.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, and `From<TrackError> for io::Error` mapping to
//! [`io::ErrorKind::InvalidData`] (encoding failures are data failures).
use std::{
    error::Error,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use flacenc::bitsink::ByteSink;

/// Error returned by [`RippedTrack::to_flac`][crate::RippedTrack::to_flac].
///
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
pub enum EncodingError {
    /// The raw track data is not a whole number of samples; carries the raw data length in bytes.
    InvalidData(usize),
    Flac(FlacError),
}

impl EncodingError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`].
    pub fn io_kind(&self) -> IoErrorKind {
        IoErrorKind::InvalidData
    }
}

impl Display for EncodingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodingError::InvalidData(len) => write!(
                f,
                "invalid raw track data: {len} bytes is not a whole number of samples"
            ),
            _ => todo!(),
        }
    }
}

impl Error for EncodingError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        todo!()
    }
}

impl From<FlacError> for EncodingError {
    fn from(value: FlacError) -> Self {
        Self::Flac(value)
    }
}

impl From<(flacenc::config::Encoder, flacenc::error::VerifyError)> for EncodingError {
    fn from(value: (flacenc::config::Encoder, flacenc::error::VerifyError)) -> Self {
        FlacError::from(value).into()
    }
}

impl From<flacenc::error::EncodeError> for EncodingError {
    fn from(value: flacenc::error::EncodeError) -> Self {
        FlacError::from(value).into()
    }
}

impl From<flacenc::error::SourceError> for EncodingError {
    fn from(value: flacenc::error::SourceError) -> Self {
        FlacError::from(value).into()
    }
}

impl From<flacenc::error::OutputError<ByteSink>> for EncodingError {
    fn from(value: flacenc::error::OutputError<ByteSink>) -> Self {
        FlacError::from(value).into()
    }
}

impl From<EncodingError> for io::Error {
    fn from(error: EncodingError) -> Self {
        io::Error::new(error.io_kind(), error)
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
        todo!()
    }
}

impl Display for FlacError {
    fn fmt(&self, _f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!()
    }
}

impl From<flacenc::error::VerifyError> for FlacError {
    fn from(value: flacenc::error::VerifyError) -> Self {
        Self::InvalidEncoder(value)
    }
}

impl From<(flacenc::config::Encoder, flacenc::error::VerifyError)> for FlacError {
    fn from(value: (flacenc::config::Encoder, flacenc::error::VerifyError)) -> Self {
        Self::InvalidEncoder(value.1)
    }
}

impl From<flacenc::error::EncodeError> for FlacError {
    fn from(encode_error: flacenc::error::EncodeError) -> Self {
        match encode_error {
            flacenc::error::EncodeError::Source(source_error) => source_error.into(),
            flacenc::error::EncodeError::Config(verify_error) => verify_error.into(),
            _ => todo!(),
        }
    }
}

impl From<flacenc::error::SourceError> for FlacError {
    fn from(value: flacenc::error::SourceError) -> Self {
        Self::InvalidData(value.to_string())
    }
}

impl From<flacenc::error::OutputError<ByteSink>> for FlacError {
    fn from(value: flacenc::error::OutputError<ByteSink>) -> Self {
        Self::OutputError(value)
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: Error + Send + Sync + 'static>() {}
        assert_traits::<EncodingError>();
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
