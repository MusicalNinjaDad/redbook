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
    error::{Error as StdError, Report},
    fmt::{self, Debug, Display},
    io::{self, RawOsError},
};

use thread_safely::Context;

use crate::{
    EncodingError, RipProgress,
    disc::{
        DiscError, TocMismatch,
        error::{TaggingError, UnknownMimeType},
    },
    drive::{DriveError, error::DeviceError},
    toc::TocError,
};

#[expect(non_camel_case_types, reason = "shadowing API Guideline naming")]
/// Well behaved Errors, which support the full range of [`std::error::Error`] functionality,
/// including methods available to `dyn Error`. See the [API Guidelines] for more details.
///
/// [API Guidelines]: https://rust-lang.github.io/api-guidelines/interoperability.html#c-good-err
pub trait C_GOOD_ERROR = StdError + Send + Sync + 'static;

/// [`std::result::Result`] alias which defaults to the crate-root [`Error`].
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Crate-root error: level 1 of the error hierarchy.
///
/// Wraps the per-domain level-2 errors (or an [`io::Error`] directly); construct via the variant,
/// a `From` conversion from a domain error or `io::Error`, or `?`.
#[derive(Debug)]
pub enum Error {
    Device(DeviceError),
    /// A [`drive`][crate::drive] domain error.
    Drive(DriveError),
    /// A [`toc`][crate::toc] domain error.
    Toc(TocError),
    /// A [`disc`][crate::disc] domain error.
    Disc(DiscError),
    /// A track-encoding domain error.
    Encoding(EncodingError),
    /// A direct io::Error occuring in a function returning [`redbook::Error`]
    IoError(io::Error),
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Toc(_) => f.write_str("TOC error"),
            Error::Disc(_) => f.write_str("disc error"),
            Error::Encoding(_) => f.write_str("track encoding error"),
            Error::Drive(_) => f.write_str("drive error"),
            Error::Device(_) => f.write_str("device error"),
            Error::IoError(_) => write!(f, "I/O error"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::Toc(error) => Some(error),
            Error::Disc(error) => Some(error),
            Error::Encoding(error) => Some(error),
            Error::Drive(error) => Some(error),
            Error::Device(error) => Some(error),
            Error::IoError(error) => Some(error),
        }
    }
}

impl From<DeviceError> for Error {
    fn from(error: DeviceError) -> Self {
        Self::Device(error)
    }
}

impl From<DriveError> for Error {
    fn from(error: DriveError) -> Self {
        Error::Drive(error)
    }
}

impl From<Context<RipProgress>> for Error {
    fn from(cx: Context<RipProgress>) -> Self {
        DriveError::from(cx).into()
    }
}

impl From<TocError> for Error {
    fn from(value: TocError) -> Self {
        Self::Toc(value)
    }
}

impl From<TocMismatch> for Error {
    fn from(value: TocMismatch) -> Self {
        DiscError::from(value).into()
    }
}

impl From<DiscError> for Error {
    fn from(error: DiscError) -> Self {
        Error::Disc(error)
    }
}

impl From<EncodingError> for Error {
    fn from(value: EncodingError) -> Self {
        Self::Encoding(value)
    }
}

impl From<UnknownMimeType> for Error {
    fn from(value: UnknownMimeType) -> Self {
        DiscError::from(value).into()
    }
}

impl From<TaggingError> for Error {
    fn from(value: TaggingError) -> Self {
        DiscError::from(value).into()
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::IoError(value)
    }
}

pub trait RedbookError: C_GOOD_ERROR + Into<Error> {
    /// The underlying <E>, if present
    fn underlying<T: StdError + 'static>(&self) -> Option<&T> {
        let e = self as &dyn StdError;
        e.sources().find_map(|err| err.downcast_ref::<T>())
    }

    /// The underlying OS error code *if present there is an underlying io::Error and it was
    /// generated from a genuine OS error*
    ///
    /// Errors which do not originate from an io::Error, which originate from a hand-made io::Error,
    /// or which originate from a 3rd-party error which has erased the initial io::Error will return
    /// `None`.
    ///
    /// If this function returns `Some(code)` you can guarantee that the error was created by
    /// calling [`io::Error::last_os_error`] or [`io::Error::from_raw_os_error`]
    fn raw_os_error(&self) -> Option<RawOsError> {
        self.underlying::<io::Error>()
            .and_then(|error| error.raw_os_error())
    }

    /// Map to an io::Error with a default ErrorKind, unless an actual io::Error exists in the
    /// source tree. TODO improve docs
    fn into_io_error(self) -> io::Error {
        let kind = self
            .underlying::<io::Error>()
            .map(io::Error::kind)
            .unwrap_or(io::ErrorKind::Other);
        io::Error::new(kind, self)
    }

    /// This simply calls `Report::new(self)` and is included solely to make discovery simpler
    /// while [`std:error::Report`] is new and may be unknown.
    fn full_message(self) -> Report<Self> {
        Report::new(self)
    }
}

impl RedbookError for Error {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<Error>();
    }

    /// TODO Test io::Error::other(_, wrapped_error), split this meaningfully (and rstest it?)
    #[test]
    fn underlying_io_error() {
        let ioerr = io::Error::other("handcrafted");
        let rberr: Error = io::Error::other("handcrafted").into();
        assert_eq!(
            rberr.underlying::<io::Error>().unwrap().kind(),
            ioerr.kind()
        );
        assert_eq!(
            rberr.underlying::<io::Error>().unwrap().to_string(),
            ioerr.to_string()
        );
        let regen = rberr.into_io_error();
        assert_eq!(regen.kind(), ioerr.kind());
        assert_eq!(regen.to_string(), "I/O error");
    }

    #[test]
    fn error_report() {
        let rberr: Error = io::Error::other("handcrafted").into();
        let rpt = rberr.full_message();
        assert_eq!(rpt.to_string(), "I/O error: handcrafted")
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
