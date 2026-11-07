//! Level-2 error for [`tagging`][crate::tagging]: the [`TaggingError`] enum.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error, data-carrying
//! variants rendered by `Display`, and `From<TaggingError> for io::Error` preserving the call
//! site's previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use crate::error::Error;

/// Error returned by [`tagging`][crate::tagging] APIs: mime-type parsing and FLAC metadata
/// read/write failures.
///
/// Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
pub enum TaggingError {
    /// The mime type of an embedded picture is not recognised; carries the mime type string.
    UnknownMimeType(Box<str>),
    /// A wrapped [`metaflac::Error`], e.g. from reading/writing a FLAC file's tags.
    Metaflac(metaflac::Error),
    /// A wrapped [`io::Error`].
    Io(io::Error),
}

impl TaggingError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it: a wrapped [`metaflac::Error`] keeps the
    /// inner `io::Error`'s kind for `Io` and maps to `InvalidData` otherwise (status quo of
    /// `VorbisTagExt::add_to`).
    pub fn io_kind(&self) -> IoErrorKind {
        match self {
            TaggingError::UnknownMimeType(_) => IoErrorKind::InvalidData,
            TaggingError::Metaflac(error) => match &error.kind {
                metaflac::ErrorKind::Io(error) => error.kind(),
                metaflac::ErrorKind::StringDecoding(_) | metaflac::ErrorKind::InvalidInput => {
                    IoErrorKind::InvalidData
                }
            },
            TaggingError::Io(error) => error.kind(),
        }
    }
}

impl Display for TaggingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaggingError::UnknownMimeType(mime) => write!(f, "unknown mime type: {mime}"),
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            TaggingError::Metaflac(_) => f.write_str("FLAC metadata error"),
            TaggingError::Io(_) => f.write_str("I/O error"),
        }
    }
}

impl StdError for TaggingError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TaggingError::Metaflac(error) => Some(error),
            TaggingError::Io(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            TaggingError::UnknownMimeType(_) => None,
        }
    }
}

impl From<metaflac::Error> for TaggingError {
    fn from(error: metaflac::Error) -> Self {
        TaggingError::Metaflac(error)
    }
}

impl From<io::Error> for TaggingError {
    fn from(error: io::Error) -> Self {
        TaggingError::Io(error)
    }
}

impl From<TaggingError> for Error {
    fn from(error: TaggingError) -> Self {
        Error::Tagging(error)
    }
}

impl From<TaggingError> for io::Error {
    fn from(error: TaggingError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Interop downgrade to [`metaflac`]'s error type: a wrapped [`metaflac::Error`] is simply
/// unwrapped; other kinds map to the closest `metaflac` semantic.
impl From<TaggingError> for metaflac::Error {
    fn from(error: TaggingError) -> Self {
        match error {
            TaggingError::Metaflac(error) => error,
            TaggingError::Io(error) => {
                metaflac::Error::new(metaflac::ErrorKind::Io(error), "io error")
            }
            TaggingError::UnknownMimeType(_) => {
                metaflac::Error::new(metaflac::ErrorKind::InvalidInput, "unknown mime type")
            }
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
        assert_traits::<TaggingError>();
    }

    #[test]
    fn display_renders_labels_and_data() {
        assert_eq!(
            TaggingError::UnknownMimeType("image/png".into()).to_string(),
            "unknown mime type: image/png"
        );
        assert_eq!(
            TaggingError::from(io::Error::new(IoErrorKind::NotFound, "gone")).to_string(),
            "I/O error"
        );
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(TaggingError::UnknownMimeType("x".into())).kind(),
            IoErrorKind::InvalidData
        );
        let inner = io::Error::new(IoErrorKind::NotFound, "file gone");
        let tagging =
            TaggingError::from(metaflac::Error::new(metaflac::ErrorKind::Io(inner), "io"));
        assert_eq!(
            io::Error::from(tagging).kind(),
            IoErrorKind::NotFound,
            "a wrapped metaflac Io error keeps the inner io::Error kind (status quo)"
        );
        let tagging = TaggingError::from(metaflac::Error::new(
            metaflac::ErrorKind::InvalidInput,
            "bad input",
        ));
        assert_eq!(
            io::Error::from(tagging).kind(),
            IoErrorKind::InvalidData,
            "a wrapped metaflac non-Io error maps to InvalidData (status quo)"
        );
    }

    #[test]
    fn source_returns_directly_wrapped_error() {
        let error = TaggingError::from(metaflac::Error::new(
            metaflac::ErrorKind::InvalidInput,
            "bad input",
        ));
        let source = StdError::source(&error).expect("wrapped metaflac error should be the source");
        assert!(source.downcast_ref::<metaflac::Error>().is_some());
        assert!(StdError::source(&TaggingError::UnknownMimeType("x".into())).is_none());
    }

    #[test]
    fn downgrades_to_metaflac_error_by_unwrapping() {
        let inner = metaflac::Error::new(metaflac::ErrorKind::InvalidInput, "bad input");
        let wrapped = TaggingError::from(inner);
        let downgraded = metaflac::Error::from(wrapped);
        assert!(matches!(downgraded.kind, metaflac::ErrorKind::InvalidInput));
        assert!(matches!(
            metaflac::Error::from(TaggingError::UnknownMimeType("x".into())).kind,
            metaflac::ErrorKind::InvalidInput
        ));
    }
}
