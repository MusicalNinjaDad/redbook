#![expect(missing_docs, reason = "TODO WIP")]

//! Crate-root error type & [`Result`] alias. Documentation of overall error handling.
//!
//! # Principles
//!
//! 1. All errors fulfil the [`C-GOOD-ERROR`][API Guidlines] expectation of implementing:
//!    `std::error::Error + Send + Sync + 'static`
//! 1. All errors build a tree and preserve fidelity by wrapping any source errors,
//!    [except where this breaks rule 1][crate::track::error::FlacError::InvalidData]
//! 1. Display implementations do not delegate to source errors
//! 1. Source errors are available via [`Error::source`]
//! 1. Additionally [`RedbookError::underlying`], and [`RedbookError::raw_os_error`] provide
//!    easy, direct access to specific elements in the error stack
//! 1. [`full_message`][RedbookError::full_message] provides easily discoverable access to [`Report`]
//!    which prints the entire error message stack
//! 1. All errors allow up-conversion via `From` where this is unambiguous. E.g.
//!    - From [`disc::error::TocMismatch`][crate::disc::error::TocMismatch] for [`Error`] returns
//!      `Error::DiscError(DiscError::TocMismatch(error))`
//!    - From [`toc::error::InvalidTrackNo`][crate::toc::error::InvalidTrackNo] for [`Error`] is
//!      not possible as this could refer to either `Error::TocError(TocError::InvalidTrackNo(error))`
//!      or `TODO #219 rehome cda parsing`
//!    - From [`toc::error::InvalidTrackNo`][crate::toc::error::InvalidTrackNo] for [`TocError`]
//!      returns `TocError::InvalidTrackNo(error)`
//!    - From [`io::Error`] for [`Error`] boxes the error into `Error::Other`
//! 1. As an exception to the above: All top-level errors (and their sibling trees) provide
//!    `From<_> for Error` - this means that [`DeviceError`] & [`MetaDataError`] convert directly to
//!    their top-level counterpart-variants, even though they can also be stored nested within other
//!    error types.
//! 1. All errors provide [`into_io_error`][RedbookError::into_io_error] but *not* `From<_> for io::Error`
//!    to ensure that  callsites can make an explicit decision regarding `ErrorKind`, whether to
//!    wrap self as a source, and how to handle errors which originate from an io::Error further
//!    down the stack.
use std::{
    error::{Error as StdError, Report, request_value},
    fmt::{self, Debug, Display},
    io::{self, RawOsError},
};

use crate::{
    disc::DiscError,
    drive::{DriveError, error::DeviceError},
    toc::TocError,
};

pub use encoding::{EncodingError, FlacError};
pub use metadata::{MetaDataError, UnknownMimeType};

mod conversions;
mod encoding;
mod metadata;

#[cfg(test)]
mod tests;

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
/// Wraps the per-domain level-2 errors (or any other [`C_GOOD_ERROR`], boxed); construct via the
/// variant, a `From` conversion from a domain error or `io::Error`, [`Error::other`], or `?`.
#[derive(Debug)]
pub enum Error {
    DeviceError(DeviceError),
    /// A [`drive`][crate::drive] domain error.
    DriveError(DriveError),
    /// A [`toc`][crate::toc] domain error.
    TocError(TocError),
    /// A [`disc`][crate::disc] domain error.
    DiscError(DiscError),
    /// A track-encoding domain error.
    EncodingError(EncodingError),
    /// An error identifying and/or parsing metadata.
    MetaDataError(MetaDataError),
    /// Any other error, boxed so the concrete type stays reachable via downcast and
    /// [`StdError::source`]
    Other(Box<dyn C_GOOD_ERROR>),
    /// A related thread cancellation request was received
    ThreadCancellation,
}

impl Error {
    /// Box any [`C_GOOD_ERROR`] into [`Error::Other`], preserving the concrete type in the
    /// source chain.
    pub fn other<E>(error: E) -> Self
    where
        E: Into<Box<dyn C_GOOD_ERROR>>,
    {
        Self::Other(error.into())
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::TocError(_) => write!(f, "TOC error"),
            Error::DiscError(_) => write!(f, "disc error"),
            Error::EncodingError(_) => write!(f, "track encoding error"),
            Error::DriveError(_) => write!(f, "drive error"),
            Error::DeviceError(_) => write!(f, "device error"),
            Error::MetaDataError(_) => write!(f, "metadata error"),
            Error::Other(_) => write!(f, "other error"),
            Error::ThreadCancellation => write!(f, "thread cancelled"),
        }
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Error::TocError(error) => Some(error),
            Error::DiscError(error) => Some(error),
            Error::EncodingError(error) => Some(error),
            Error::DriveError(error) => Some(error),
            Error::DeviceError(error) => Some(error),
            Error::MetaDataError(error) => Some(error),
            Error::Other(error) => Some(&**error),
            Error::ThreadCancellation => None,
        }
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            Error::Other(_) => {
                request.provide_value(io::ErrorKind::Other);
            }
            Error::ThreadCancellation => {
                request.provide_value(io::ErrorKind::Interrupted);
            }
            _ => (),
        }
    }
}

pub trait RedbookError: C_GOOD_ERROR + Sized {
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

    /// Convert into an [`io::Error`], wrapping `self` as the payload so the full typed error
    /// tree remains reachable via downcast and [`StdError::source`].
    ///
    /// The [`io::ErrorKind`] is taken from an underlying [`io::Error`] in the source chain when
    /// one exists (preserving e.g. `ErrorKind::NotFound` or the kind of a genuine OS error);
    /// otherwise it defaults to [`io::ErrorKind::Other`]. Call sites which need a more specific
    /// kind should construct the [`io::Error`] themselves, e.g.
    /// `io::Error::new(io::ErrorKind::InvalidData, error)`.
    fn into_io_error(self) -> io::Error {
        let kind = self
            .underlying::<io::Error>()
            .map(io::Error::kind)
            .or_else(|| {
                let e = &self as &dyn StdError;
                e.sources()
                    .filter_map(request_value::<io::ErrorKind>)
                    .last()
            })
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
