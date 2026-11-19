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

impl From<flacenc::error::OutputError<ByteSink>> for EncodingError {
    fn from(error: flacenc::error::OutputError<ByteSink>) -> Self {
        FlacError::from(error).into()
    }
}

#[derive(Debug, Clone)]
pub enum FlacError {
    InvalidEncoder(flacenc::error::VerifyError),
    OutputError(flacenc::error::OutputError<ByteSink>),
}

impl Error for FlacError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            FlacError::InvalidEncoder(error) => Some(error),
            FlacError::OutputError(error) => Some(error),
        }
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            FlacError::InvalidEncoder(_) => {
                request.provide_value(io::ErrorKind::Other);
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
            flacenc::error::EncodeError::Config(verify_error) => verify_error.into(),
            _ => todo!(),
        }
    }
}

impl From<flacenc::error::OutputError<ByteSink>> for FlacError {
    fn from(error: flacenc::error::OutputError<ByteSink>) -> Self {
        Self::OutputError(error)
    }
}

impl RedbookError for FlacError {}
