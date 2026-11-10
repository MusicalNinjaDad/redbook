#![expect(missing_docs, reason = "TODO WIP")]

//! Crate-root error type & [`Result`] alias: level 1 of redbook's error hierarchy.
//!
//! # Hierarchy
//!
//! Errors form a 2-3 level tree:
//!
//! 1. [`Error`] - the crate-root enum, re-exported as [`redbook::Error`][crate::Error]. Used by
//!    public APIs which span multiple domains; the re-exported [`Result`] alias defaults to it.
//! 2. Domain errors (e.g. `toc::TocError`, `drive::DriveError`) - one enum per module. Most public
//!    signatures return `Result<_, DomainError>` directly (or a domain `Result` alias such as
//!    [`toc::TocResult`][crate::toc::TocResult]).
//! 3. Leaf errors - variants of the domain enums, or dedicated leaf structs (e.g.
//!    [`toc::error::InvalidTrackNo`]) which carry the offending value. Variants wrapping a foreign
//!    error hold it in a single tuple field (e.g. `Io(io::Error)`, `CdToc(cdtoc::TocError)`).
//!
//! # Conventions
//!
//! - All error types are hand-rolled: they implement [`std::error::Error`] and are
//!   `Send + Sync + 'static` (C-GOOD-ERR, compile-time asserted in tests). Error enums are
//!   deliberately *not* `#[non_exhaustive]`: extending them is a semver-breaking change, preferred
//!   over forcing `_ =>` arms downstream (issue #197).
//! - `source()` returns the *directly wrapped* error, so drilling down via `source()` walks the
//!   whole chain (a wrapped `io::Error` keeps its original [`io::ErrorKind`] / raw OS error all the
//!   way down). A variant which wraps an underlying error is *not* also rendered by [`Display`]
//!   (issue #198): either `source()` or `Display`, never both. Variants which carry plain data
//!   (not an error) instead render the data in `Display` and return `None` from `source()`.
//! - Context is the *data which led to the error*, stored in the variant itself (issue #193) -
//!   there is no stringly-typed `.context(...)` API. Constructors which capture a wrapped value
//!   allow closure-free `.map_err(MyError::new)`, and `From` conversions make `?` work directly at
//!   call sites (issues #190, #194).
//! - Every error type implements `From<This> for io::Error` for interop; the [`io::ErrorKind`] is
//!   chosen to match the call site's previous behaviour, and the original error remains available
//!   via [`io::Error::get_ref`]. Relevant cases also implement `From<This> for other_crate::Error`
//!   for the glued crates (`cd_toc`, `musicbrainz_rs`, `flacenc`, `metaflac`), mainly unwrapping a
//!   wrapped foreign error.
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io,
};

use crate::{
    disc::{DiscError, TocMismatch},
    drive::{self, DriveError, error::DeviceError},
};

/// [`std::result::Result`] alias which defaults to the crate-root [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Crate-root error: level 1 of the error hierarchy.
///
/// Wraps the per-domain level-2 errors (or an [`io::Error`] directly); construct via the variant,
/// a `From` conversion from a domain error or `io::Error`, or `?`.
#[derive(Debug)]
pub enum Error {
    /// A [`toc`][crate::toc] domain error.
    Toc(crate::toc::TocError),
    /// A [`disc`][crate::disc] domain error.
    Disc(DiscError),
    /// A track-encoding domain error.
    Track(crate::track::TrackError),
    /// A [`drive`][crate::drive] domain error.
    Drive(DriveError),
    Device(drive::error::DeviceError),
}

impl Error {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> io::ErrorKind {
        match self {
            Error::Toc(error) => error.io_kind(),
            Error::Disc(error) => error.io_kind(),
            Error::Track(error) => error.io_kind(),
            Error::Drive(error) => error.io_kind(),
            Error::Device(error) => error.io_kind(),
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            Error::Toc(_) => f.write_str("TOC error"),
            Error::Disc(_) => f.write_str("disc error"),
            Error::Track(_) => f.write_str("track encoding error"),
            Error::Drive(_) => f.write_str("drive error"),
            Error::Device(_) => f.write_str("device error"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Toc(error) => Some(error),
            Error::Disc(error) => Some(error),
            Error::Track(error) => Some(error),
            Error::Drive(error) => Some(error),
            Error::Device(error) => Some(error),
        }
    }
}

impl From<DriveError> for Error {
    fn from(error: DriveError) -> Self {
        Error::Drive(error)
    }
}

///TODO Why aren't similar impls here??
impl From<DeviceError> for Error {
    fn from(error: DeviceError) -> Self {
        Self::Device(error)
    }
}

impl From<DiscError> for Error {
    fn from(error: DiscError) -> Self {
        Error::Disc(error)
    }
}

impl From<TocMismatch> for Error {
    fn from(value: TocMismatch) -> Self {
        DiscError::from(value).into()
    }
}

/// TODO - NO! If an actual io::Error is there, must return that!
impl From<Error> for io::Error {
    fn from(error: Error) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<Error>();
    }

    #[cfg(false)]
    #[test]
    fn display_renders_variant_label_not_wrapped_error() {
        let error = Error::from(io::Error::new(io::ErrorKind::NotFound, "no disc"));
        assert_eq!(error.to_string(), "I/O error");
        assert_eq!(
            Error::from(crate::toc::TocError::NotFound).to_string(),
            "TOC error"
        );
    }

    // TODO: #200 Review cfg(false) tests to identify where we need new ones
    #[cfg(false)]
    #[test]
    fn source_returns_directly_wrapped_io_error() {
        let raw = io::Error::from_raw_os_error(22); // ERROR_NOT_READY
        let error = Error::from(raw);
        let source = StdError::source(&error).expect("the wrapped io::Error should be the source");
        let inner = source
            .downcast_ref::<io::Error>()
            .expect("source should be the io::Error directly");
        assert_eq!(inner.raw_os_error(), Some(22));
    }

    #[cfg(false)]
    #[test]
    fn io_error_conversion_preserves_kind_and_downcast() {
        let raw = io::Error::from_raw_os_error(22); // ERROR_NOT_READY on Windows
        let expected_kind = raw.kind();
        let io_error = io::Error::from(Error::from(raw));
        assert_eq!(io_error.kind(), expected_kind);
        let payload = io_error.get_ref().expect("io::Error should hold a payload");
        let error = payload
            .downcast_ref::<Error>()
            .expect("payload should be the original Error");
        let inner = StdError::source(error)
            .and_then(|source| source.downcast_ref::<io::Error>())
            .expect("source chain should reach the original io::Error");
        assert_eq!(inner.raw_os_error(), Some(22));
    }
}
