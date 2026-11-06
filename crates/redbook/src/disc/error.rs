//! Level-2 error for [`disc`][crate::disc]: [`DiscError`] with leaf kinds in [`DiscErrorKind`].
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `#[non_exhaustive]`, optional `.context(...)` chaining which is
//! omitted from `Display` when absent, `source()` returning the directly wrapped type, and
//! `From<DiscError> for io::Error` preserving the call site's previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
};

use thread_safely::Context;

use crate::error::{Error, ErrorKind};

/// Error returned by [`Disc`][crate::Disc] APIs: metadata lookup, cover art & validation
/// failures.
///
/// Construct simple variants via their constructor (usable directly in
/// `.map_err(DiscError::invalid_input)`) and attach optional context with
/// [`DiscError::context`]. Wrapped foreign errors convert via `From` / `?`.
#[derive(Debug)]
#[non_exhaustive]
pub struct DiscError {
    kind: DiscErrorKind,
    context: Option<Box<str>>,
}

impl DiscError {
    /// The detailed kind of this error.
    pub const fn kind(&self) -> &DiscErrorKind {
        &self.kind
    }

    /// Attach optional context, rendered by [`Display`] when present.
    pub fn context(mut self, context: impl Display) -> Self {
        self.context = Some(context.to_string().into_boxed_str());
        self
    }

    /// Input data is invalid, e.g. tracks which do not match the TOC.
    pub const fn invalid_input() -> Self {
        Self {
            kind: DiscErrorKind::InvalidInput,
            context: None,
        }
    }

    /// No MusicBrainz release was found / selected.
    pub const fn no_releases() -> Self {
        Self {
            kind: DiscErrorKind::NoReleases,
            context: None,
        }
    }

    /// The rip was cancelled via its [`Context`].
    pub const fn cancelled() -> Self {
        Self {
            kind: DiscErrorKind::Cancelled,
            context: None,
        }
    }

    /// A wrapped [`io::Error`].
    pub fn io(error: io::Error) -> Self {
        Self {
            kind: DiscErrorKind::Io(error),
            context: None,
        }
    }

    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match &self.kind {
            DiscErrorKind::InvalidInput => IoErrorKind::InvalidInput,
            DiscErrorKind::NoReleases | DiscErrorKind::MusicBrainz(_) | DiscErrorKind::Http(_) => {
                IoErrorKind::Other
            }
            DiscErrorKind::Cancelled => IoErrorKind::Interrupted,
            DiscErrorKind::Io(error) => error.kind(),
        }
    }
}

impl Display for DiscError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.context {
            Some(context) => write!(f, "{}: {context}", self.kind),
            None => Display::fmt(&self.kind, f),
        }
    }
}

impl StdError for DiscError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(&self.kind)
    }
}

impl From<DiscErrorKind> for DiscError {
    fn from(kind: DiscErrorKind) -> Self {
        Self {
            kind,
            context: None,
        }
    }
}

impl From<musicbrainz_rs::ApiEndpointError> for DiscError {
    fn from(error: musicbrainz_rs::ApiEndpointError) -> Self {
        Self {
            kind: DiscErrorKind::MusicBrainz(Box::new(error)),
            context: None,
        }
    }
}

impl From<reqwest::Error> for DiscError {
    fn from(error: reqwest::Error) -> Self {
        Self {
            kind: DiscErrorKind::Http(error),
            context: None,
        }
    }
}

impl<R> From<Context<R>> for DiscError {
    fn from(_: Context<R>) -> Self {
        Self::cancelled()
    }
}

impl From<io::Error> for DiscError {
    fn from(error: io::Error) -> Self {
        Self::io(error)
    }
}

impl From<DiscError> for Error {
    fn from(error: DiscError) -> Self {
        Error::from(ErrorKind::Disc(error))
    }
}

impl From<DiscError> for io::Error {
    fn from(error: DiscError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Leaf kinds of [`DiscError`]: level 3 of the hierarchy. Variants wrapping a foreign error hold
/// it in a single tuple field and return it directly from `source()`.
#[derive(Debug)]
#[non_exhaustive]
pub enum DiscErrorKind {
    /// Input data is invalid, e.g. tracks which do not match the TOC.
    InvalidInput,
    /// No MusicBrainz release was found / selected.
    NoReleases,
    /// The rip was cancelled via its [`Context`][thread_safely::Context].
    Cancelled,
    /// A wrapped [`musicbrainz_rs`] error, e.g. from fetching disc metadata. Boxed because the
    /// snafu-generated error is too large to keep the `Err` variant small
    /// (`clippy::result_large_err`).
    MusicBrainz(Box<musicbrainz_rs::ApiEndpointError>),
    /// A wrapped [`reqwest`] error, e.g. from downloading cover art.
    Http(reqwest::Error),
    /// A wrapped [`io::Error`].
    Io(io::Error),
}

impl Display for DiscErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiscErrorKind::InvalidInput => f.write_str("invalid input"),
            DiscErrorKind::NoReleases => f.write_str("no releases found"),
            DiscErrorKind::Cancelled => f.write_str("thread cancellation requested by controller"),
            DiscErrorKind::MusicBrainz(error) => Display::fmt(error, f),
            DiscErrorKind::Http(error) => Display::fmt(error, f),
            DiscErrorKind::Io(error) => Display::fmt(error, f),
        }
    }
}

impl StdError for DiscErrorKind {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            DiscErrorKind::MusicBrainz(error) => Some(error),
            DiscErrorKind::Http(error) => Some(error),
            DiscErrorKind::Io(error) => Some(error),
            DiscErrorKind::InvalidInput | DiscErrorKind::NoReleases | DiscErrorKind::Cancelled => {
                None
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
        assert_traits::<DiscError>();
        assert_traits::<DiscErrorKind>();
    }

    #[test]
    fn display_omits_missing_context() {
        assert_eq!(DiscError::no_releases().to_string(), "no releases found");
    }

    #[test]
    fn display_includes_context_when_provided() {
        let error = DiscError::invalid_input().context("track 3 not in TOC");
        assert_eq!(error.to_string(), "invalid input: track 3 not in TOC");
    }

    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(DiscError::invalid_input()).kind(),
            IoErrorKind::InvalidInput
        );
        assert_eq!(
            io::Error::from(DiscError::no_releases()).kind(),
            IoErrorKind::Other
        );
        assert_eq!(
            io::Error::from(DiscError::cancelled()).kind(),
            IoErrorKind::Interrupted
        );
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(DiscError::no_releases());
        assert!(matches!(error.kind(), ErrorKind::Disc(_)));
    }
}
