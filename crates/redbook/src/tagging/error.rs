//! Level-2 error for [`tagging`][crate::tagging]: [`TaggingError`] with leaf kinds in
//! [`TaggingErrorKind`].
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `#[non_exhaustive]`, optional `.context(...)` chaining which is
//! omitted from `Display` when absent, `source()` returning the directly wrapped type, and
//! `From<TaggingError> for io::Error` preserving the call site's previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use crate::error::{Error, ErrorKind};

/// Error returned by [`tagging`][crate::tagging] APIs: mime-type parsing and FLAC metadata
/// read/write failures.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(TaggingError::invalid_data)`) and attach optional context with
/// [`TaggingError::context`]. Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct TaggingError {
    kind: TaggingErrorKind,
    context: Option<Box<str>>,
}

impl TaggingError {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &TaggingErrorKind {
        &self.kind
    }

    /// Attach optional context, rendered by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// The tag data is invalid, e.g. an unknown mime type.
    pub const fn invalid_data() -> Self {
        Self {
            kind: TaggingErrorKind::InvalidData,
            context: None,
        }
    }

    /// A wrapped [`io::Error`].
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: TaggingErrorKind::Io(error),
            context: None,
        }
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it: a wrapped [`metaflac::Error`] keeps the
    /// inner `io::Error`'s kind for `Io` and maps to `InvalidData` otherwise (status quo of
    /// `VorbisTagExt::add_to`).
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            TaggingErrorKind::InvalidData => IoErrorKind::InvalidData,
            TaggingErrorKind::Metaflac(error) => match &error.kind {
                metaflac::ErrorKind::Io(error) => error.kind(),
                metaflac::ErrorKind::StringDecoding(_) | metaflac::ErrorKind::InvalidInput => {
                    IoErrorKind::InvalidData
                }
            },
            TaggingErrorKind::Io(error) => error.kind(),
        }
    }
}

impl Display for TaggingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for TaggingError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<TaggingErrorKind> for TaggingError {
    fn from(kind: TaggingErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<metaflac::Error> for TaggingError {
    fn from(error: metaflac::Error) -> Self {
        Self {
            kind: TaggingErrorKind::Metaflac(error),
            context: None,
        }
    }
}

impl From<io::Error> for TaggingError {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<TaggingError> for Error {
    fn from(error: TaggingError) -> Self {
        Error::from(ErrorKind::Tagging(error))
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
        match error.kind {
            TaggingErrorKind::Metaflac(error) => error,
            TaggingErrorKind::Io(error) => {
                metaflac::Error::new(metaflac::ErrorKind::Io(error), "io error")
            }
            TaggingErrorKind::InvalidData => {
                metaflac::Error::new(metaflac::ErrorKind::InvalidInput, "invalid tag data")
            }
        }
    }
}

/// Leaf kinds of [`TaggingError`]: level 3 of the hierarchy. Variants wrapping a foreign error
/// hold it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum TaggingErrorKind {
    /// The tag data is invalid, e.g. an unknown mime type.
    InvalidData,
    /// A wrapped [`metaflac::Error`], e.g. from reading/writing a FLAC file's tags.
    Metaflac(metaflac::Error),
    /// A wrapped [`io::Error`].
    Io(io::Error),
}

impl Display for TaggingErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaggingErrorKind::InvalidData => f.write_str("invalid tag data"),
            TaggingErrorKind::Metaflac(error) => Display::fmt(error, f),
            TaggingErrorKind::Io(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for TaggingErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TaggingErrorKind::Metaflac(error) => Some(error),
            TaggingErrorKind::Io(error) => Some(error),
            TaggingErrorKind::InvalidData => None,
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
        assert_traits::<TaggingErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(TaggingError::invalid_data().to_string(), "invalid tag data");
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = TaggingError::invalid_data().context("unknown mime type");
        assert_eq!(error.to_string(), "invalid tag data: unknown mime type");
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(TaggingError::invalid_data()).kind(),
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
    fn downgrades_to_metaflac_error_by_unwrapping() {
        let inner = metaflac::Error::new(metaflac::ErrorKind::InvalidInput, "bad input");
        let wrapped = TaggingError::from(inner);
        let downgraded = metaflac::Error::from(wrapped);
        assert!(matches!(downgraded.kind, metaflac::ErrorKind::InvalidInput));
        assert!(matches!(
            metaflac::Error::from(TaggingError::invalid_data()).kind,
            metaflac::ErrorKind::InvalidInput
        ));
    }
}
