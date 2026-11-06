//! Level-2 error for track encoding: [`TrackError`] with leaf kinds in [`TrackErrorKind`].
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `#[non_exhaustive]`, optional `.context(...)` chaining which is
//! omitted from `Display` when absent, `source()` returning the directly wrapped type, and
//! `From<TrackError> for io::Error` mapping to [`io::ErrorKind::InvalidData`] (encoding failures
//! are data failures).
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use flacenc::{
    bitsink::ByteSink,
    error::{EncodeError, OutputError},
};

use crate::error::{Error, ErrorKind};

/// Error returned by [`RippedTrack::to_flac`][crate::RippedTrack::to_flac].
///
/// Construct simple variants via their constructor and attach optional context with
/// [`TrackError::context`]. Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct TrackError {
    kind: TrackErrorKind,
    context: Option<Box<str>>,
}

impl TrackError {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &TrackErrorKind {
        &self.kind
    }

    /// Attach optional context, rendered by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// The raw track data is invalid, e.g. not a whole number of samples.
    pub const fn invalid_data() -> Self {
        Self {
            kind: TrackErrorKind::InvalidData,
            context: None,
        }
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`].
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            TrackErrorKind::InvalidData | TrackErrorKind::Encode(_) | TrackErrorKind::Output(_) => {
                IoErrorKind::InvalidData
            }
        }
    }
}

impl Display for TrackError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for TrackError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<TrackErrorKind> for TrackError {
    fn from(kind: TrackErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<EncodeError> for TrackError {
    fn from(error: EncodeError) -> Self {
        // flacenc's EncodeError is !Send (it can hold an Rc<dyn Error>), so it cannot be wrapped
        // in a Send + Sync error (C-GOOD-ERR); its message is retained instead.
        Self {
            kind: TrackErrorKind::Encode(error.to_string()),
            context: None,
        }
    }
}

impl From<OutputError<ByteSink>> for TrackError {
    fn from(error: OutputError<ByteSink>) -> Self {
        Self {
            kind: TrackErrorKind::Output(error),
            context: None,
        }
    }
}

impl From<TrackError> for Error {
    fn from(error: TrackError) -> Self {
        Error::from(ErrorKind::Track(error))
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
        match error.kind {
            TrackErrorKind::Encode(message) => {
                EncodeError::Config(flacenc::error::VerifyError::new("track", &message))
            }
            TrackErrorKind::InvalidData | TrackErrorKind::Output(_) => EncodeError::Config(
                flacenc::error::VerifyError::new("track", "invalid raw track data"),
            ),
        }
    }
}

/// Leaf kinds of [`TrackError`]: level 3 of the hierarchy. Variants wrapping a foreign error hold
/// it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum TrackErrorKind {
    /// The raw track data is invalid, e.g. not a whole number of samples.
    InvalidData,
    /// A [`flacenc`] encode failure. flacenc's `EncodeError` is !Send, so only its message is
    /// retained (C-GOOD-ERR requires error types to be `Send + Sync`).
    Encode(String),
    /// A wrapped [`flacenc`] output error.
    Output(OutputError<ByteSink>),
}

impl Display for TrackErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TrackErrorKind::InvalidData => f.write_str("invalid raw track data"),
            TrackErrorKind::Encode(message) => write!(f, "flac encode failed: {message}"),
            TrackErrorKind::Output(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for TrackErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TrackErrorKind::Output(error) => Some(error),
            TrackErrorKind::InvalidData | TrackErrorKind::Encode(_) => None,
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
        assert_traits::<TrackErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(
            TrackError::invalid_data().to_string(),
            "invalid raw track data"
        );
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = TrackError::invalid_data().context("odd byte count");
        assert_eq!(error.to_string(), "invalid raw track data: odd byte count");
    }

    #[test]
    fn io_error_conversion_maps_to_invalid_data() {
        assert_eq!(
            io::Error::from(TrackError::invalid_data()).kind(),
            IoErrorKind::InvalidData
        );
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(TrackError::invalid_data());
        assert!(matches!(error.kind(), ErrorKind::Track(_)));
    }
}
