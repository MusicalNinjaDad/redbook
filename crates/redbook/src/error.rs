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
//! 1. All errors provide [`into_io_error`][RedbookError::into_io_error] but *not* `From<_> for io::Error`
//!    to ensure that  callsites can make an explicit decision regarding `ErrorKind`, whether to
//!    wrap self as a source, and how to handle errors which originate from an io::Error further
//!    down the stack.
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
        error::{IoError, TaggingError, UnknownMimeType},
    },
    drive::{
        DriveError,
        error::{DeviceError, InvalidBufferSize},
    },
    frame::InvalidByteSize,
    toc::{TocError, error::InvalidOrder},
    track::error::FlacError,
    win::WinError,
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
    /// Any other error, boxed so the concrete type stays reachable via downcast and
    /// [`StdError::source`]
    Other(Box<dyn C_GOOD_ERROR>),
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
            Error::Other(_) => write!(f, "other error"),
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
            Error::Other(error) => Some(&**error),
        }
    }
}

impl From<DeviceError> for Error {
    fn from(error: DeviceError) -> Self {
        Self::DeviceError(error)
    }
}

impl From<InvalidBufferSize> for Error {
    fn from(error: InvalidBufferSize) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<InvalidByteSize> for Error {
    fn from(error: InvalidByteSize) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<WinError> for Error {
    fn from(error: WinError) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<DriveError> for Error {
    fn from(error: DriveError) -> Self {
        Self::DriveError(error)
    }
}

impl From<crossbeam::channel::SendError<RipProgress>> for Error {
    fn from(error: crossbeam::channel::SendError<RipProgress>) -> Self {
        DriveError::from(error).into()
    }
}

impl From<Context<RipProgress>> for Error {
    fn from(cx: Context<RipProgress>) -> Self {
        DriveError::from(cx).into()
    }
}

impl From<TocError> for Error {
    fn from(error: TocError) -> Self {
        Self::TocError(error)
    }
}

impl From<cdtoc::TocError> for Error {
    fn from(error: cdtoc::TocError) -> Self {
        TocError::from(error).into()
    }
}

impl From<InvalidOrder> for Error {
    fn from(error: InvalidOrder) -> Self {
        TocError::from(error).into()
    }
}

impl From<DiscError> for Error {
    fn from(error: DiscError) -> Self {
        Self::DiscError(error)
    }
}

impl From<TocMismatch> for Error {
    fn from(error: TocMismatch) -> Self {
        DiscError::from(error).into()
    }
}

impl From<TaggingError> for Error {
    fn from(error: TaggingError) -> Self {
        DiscError::from(error).into()
    }
}

impl From<UnknownMimeType> for Error {
    fn from(error: UnknownMimeType) -> Self {
        DiscError::from(error).into()
    }
}

impl From<metaflac::Error> for Error {
    fn from(error: metaflac::Error) -> Self {
        DiscError::from(error).into()
    }
}

impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        DiscError::from(error).into()
    }
}

impl From<musicbrainz_rs::ApiEndpointError> for Error {
    fn from(error: musicbrainz_rs::ApiEndpointError) -> Self {
        DiscError::from(error).into()
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        DiscError::from(error).into()
    }
}

impl From<Context<!>> for Error {
    fn from(cx: Context<!>) -> Self {
        DiscError::from(cx).into()
    }
}

impl From<EncodingError> for Error {
    fn from(error: EncodingError) -> Self {
        Self::EncodingError(error)
    }
}

impl From<FlacError> for Error {
    fn from(error: FlacError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::VerifyError> for Error {
    fn from(error: flacenc::error::VerifyError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<(flacenc::config::Encoder, flacenc::error::VerifyError)> for Error {
    fn from(error: (flacenc::config::Encoder, flacenc::error::VerifyError)) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::EncodeError> for Error {
    fn from(error: flacenc::error::EncodeError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::SourceError> for Error {
    fn from(error: flacenc::error::SourceError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::OutputError<flacenc::bitsink::ByteSink>> for Error {
    fn from(error: flacenc::error::OutputError<flacenc::bitsink::ByteSink>) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::other(error)
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

    /// TODO better docs here & update into_io_error
    ///
    /// Override this to provide custom logic for into_io_error. Don't override into_io_error
    fn fallback_io_errorkind(&self) -> io::ErrorKind {
        io::ErrorKind::Other
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
            .unwrap_or_else(|| RedbookError::fallback_io_errorkind(&self));
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

    #[test]
    fn all_tree_error_types_implement_redbook_error() {
        fn assert_redbook<T: RedbookError>() {}
        assert_redbook::<Error>();
        assert_redbook::<DeviceError>();
        assert_redbook::<DriveError>();
        assert_redbook::<InvalidBufferSize>();
        assert_redbook::<TocError>();
        assert_redbook::<crate::toc::error::InvalidTrackNo>();
        assert_redbook::<InvalidOrder>();
        assert_redbook::<DiscError>();
        assert_redbook::<TocMismatch>();
        assert_redbook::<TaggingError>();
        assert_redbook::<UnknownMimeType>();
        assert_redbook::<IoError>();
        assert_redbook::<EncodingError>();
        assert_redbook::<FlacError>();
        assert_redbook::<crate::win::CdaError>();
        assert_redbook::<InvalidByteSize>();
        assert_redbook::<crate::frame::InvalidFrame>();
        assert_redbook::<crate::frame::ParseByteSizeError>();
        assert_redbook::<crate::hex::ParseHexError>();
    }

    #[test]
    fn subtree_errors_upconvert_via_from() {
        // grandchild of Error via DeviceError
        let error = Error::from(InvalidBufferSize::TooSmall {
            provided: 1,
            required: 2,
        });
        assert!(matches!(
            error,
            Error::DeviceError(DeviceError::InvalidBufferSize(
                InvalidBufferSize::TooSmall { .. }
            ))
        ));
        // great-grandchild of Error: the same impl works verbatim
        let error = Error::from(InvalidByteSize::new(u64::MAX));
        assert!(matches!(
            error,
            Error::DeviceError(DeviceError::InvalidBufferSize(
                InvalidBufferSize::InvalidByteSize(_)
            ))
        ));
        // grandchild of Error via TocError
        let error = Error::from(cdtoc::TocError::NoAudio);
        assert!(matches!(error, Error::TocError(TocError::CdToc(_))));
        // great-grandchild of Error via DiscError
        let error = Error::from(UnknownMimeType::from("application/x-unknown"));
        assert!(matches!(
            error,
            Error::DiscError(DiscError::TaggingError(TaggingError::UnknownMimeType(_)))
        ));
        // grandchild of Error via EncodingError
        let error = Error::from(FlacError::InvalidData("boom".to_string()));
        assert!(matches!(
            error,
            Error::EncodingError(EncodingError::FlacError(FlacError::InvalidData(_)))
        ));
    }

    #[test]
    fn source_chain_preserves_fidelity_from_root() {
        use crate::toc::error::InvalidTrackNo;

        let error = Error::from(TocError::from(InvalidTrackNo::new(100_u8)));
        // rule 2: each layer renders only its own message
        assert_eq!(error.to_string(), "TOC error");
        let toc = StdError::source(&error)
            .and_then(|s| s.downcast_ref::<TocError>())
            .expect("source should be the TocError");
        assert_eq!(toc.to_string(), "invalid track number in TOC");
        // rule 3: the wrapped leaf is reachable via `source()` ...
        let leaf = StdError::source(toc)
            .and_then(|s| s.downcast_ref::<InvalidTrackNo>())
            .expect("source should be the InvalidTrackNo");
        assert_eq!(leaf.to_string(), "invalid track number: 0x0064");
        // ... and via the RedbookError helper walking the whole chain
        assert!(error.underlying::<InvalidTrackNo>().is_some());
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
    fn other_boxes_any_c_good_error_with_fidelity() {
        let error = Error::other(std::fmt::Error);
        assert!(matches!(error, Error::Other(_)));
        assert_eq!(error.to_string(), "other error");
        let source = StdError::source(&error).expect("boxed error should be the source");
        assert!(source.downcast_ref::<std::fmt::Error>().is_some());
        assert!(error.underlying::<std::fmt::Error>().is_some());
        // io::Errors up-convert into the same variant but keep their label
        let io_error: Error = io::Error::other("handcrafted").into();
        assert!(matches!(io_error, Error::Other(_)));
        assert_eq!(io_error.to_string(), "other error");
    }

    #[test]
    fn into_io_error_uses_better_kind_than_other_where_defined() {
        use crate::toc::TocError;
        // per-type overrides restore the status-quo kinds
        assert_eq!(
            TocError::NotFound.into_io_error().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(
            TocError::from(cdtoc::TocError::NoAudio)
                .into_io_error()
                .kind(),
            io::ErrorKind::InvalidData
        );
        assert_eq!(
            DriveError::ZeroLengthTrack.into_io_error().kind(),
            io::ErrorKind::UnexpectedEof
        );
        assert_eq!(
            EncodingError::InvalidData(2353).into_io_error().kind(),
            io::ErrorKind::InvalidData
        );
        // errors without an io::Error in the chain and without an override default to Other
        assert_eq!(
            Error::from(DiscError::NoReleases).into_io_error().kind(),
            io::ErrorKind::Other
        );
    }

    #[test]
    fn into_io_error_preserves_kind_of_underlying_io_error() {
        // an io::Error anywhere in the source chain wins over the per-type fallback kind
        let raw = io::Error::from_raw_os_error(5);
        let expected_kind = raw.kind();
        let error = DeviceError::Other(raw);
        let io_error = error.into_io_error();
        assert_eq!(io_error.kind(), expected_kind);
        // the typed error is still the payload, so the tree (and the raw OS code) survives
        let payload = io_error
            .get_ref()
            .and_then(|e| e.downcast_ref::<DeviceError>())
            .expect("payload should be the DeviceError");
        assert_eq!(payload.raw_os_error(), Some(5));
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
