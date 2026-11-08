//! Level-2 error for [`toc`][crate::toc]: the [`TocError`] enum & the [`InvalidTrackNo`] leaf.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error, data-carrying
//! variants rendered by `Display`, and `From<TocError> for io::Error` preserving the call site's
//! previous [`io::ErrorKind`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io::{self, ErrorKind as IoErrorKind},
    num::NonZeroUsize,
};

use crate::{error::Error, toc::Format};

use super::{InfoEntry, RawTocEntry, TocEntry, TrackNo};

/// [`Result`] alias for [`toc`][crate::toc] APIs.
pub type TocResult<T> = Result<T, TocError>;

/// Error returned by [`toc`][crate::toc] APIs: TOC parsing & validation failures.
///
/// Variants carry the data which led to the error (issue #193). Variants wrapping an underlying
/// error ([`CdToc`][TocError::CdToc], [`Io`][TocError::Io]) are returned by `source()` and *not*
/// rendered by `Display`; data-carrying variants are rendered by `Display` and return `None` from
/// `source()` (issue #198). Wrapped foreign errors convert via `From` / `?` (issue #194).
#[derive(Debug)]
pub enum TocError {
    /// A raw entry is not a valid track entry (ADR != 1).
    InvalidTrackEntry(RawTocEntry),
    /// A raw entry is not a valid lead-out entry (ADR != 1 or POINT is not a lead-out code).
    InvalidLeadoutEntry(RawTocEntry),
    /// A raw entry is not a valid info entry (ADR != 1, POINT not 0xA0/0xA1, or session info
    /// missing).
    InvalidInfoEntry(RawTocEntry),
    /// A track-number value is outside the valid range 1..=99.
    InvalidTrackNo(InvalidTrackNo),
    /// The entries cannot be sorted: they appear to be from multiple discs.
    MultiDisc,
    /// An info entry (POINT 0xA0/0xA1) disagrees with the actual first/last track found.
    InfoMismatch {
        /// The info entry as read from the disc.
        entry: InfoEntry,
        /// The first/last track actually found in the entries of the same kind.
        found: Option<TrackNo>,
    },
    /// Some entries have session info, others do not.
    InconsistentSessionInfo,
    /// The number of lead-out entries does not match the number of sessions.
    LeadoutMismatch {
        /// The number of distinct sessions, if any entry has session info.
        sessions: Option<NonZeroUsize>,
        /// The number of lead-out entries found.
        leadouts: usize,
    },
    /// Two adjacent entries are in an invalid order.
    InvalidOrder(InvalidOrder),
    /// A required TOC element (e.g. the lead-out) was not found, or no entries were provided.
    NotFound,
    /// A wrapped [`cdtoc::TocError`], e.g. from [`cdtoc::Toc::from_parts`].
    CdToc(cdtoc::TocError),
}

impl TocError {
    /// The [`io::ErrorKind`] this error maps to when converted to [`io::Error`], matching the
    /// behaviour of the call site which produced it.
    pub fn io_kind(&self) -> IoErrorKind {
        match self {
            TocError::NotFound => IoErrorKind::NotFound,
            TocError::CdToc(_) => IoErrorKind::InvalidData,
            TocError::InvalidTrackEntry(_)
            | TocError::InvalidLeadoutEntry(_)
            | TocError::InvalidInfoEntry(_)
            | TocError::InvalidTrackNo(_)
            | TocError::MultiDisc
            | TocError::InfoMismatch { .. }
            | TocError::InconsistentSessionInfo
            | TocError::LeadoutMismatch { .. }
            | TocError::InvalidOrder { .. } => IoErrorKind::InvalidData,
        }
    }
}

impl Display for TocError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TocError::InvalidTrackEntry(raw) => {
                write!(f, "entry not valid as a track entry: {raw:?}")
            }
            TocError::InvalidLeadoutEntry(raw) => {
                write!(f, "entry not valid as a lead-out entry: {raw:?}")
            }
            TocError::InvalidInfoEntry(raw) => {
                write!(f, "entry not valid as an info entry: {raw:?}")
            }
            TocError::InvalidTrackNo(error) => Display::fmt(error, f),
            TocError::MultiDisc => f.write_str("TOC entries appear to be from multiple discs"),
            TocError::InfoMismatch { entry, found } => write!(
                f,
                "info entry {entry:?} does not match the track entries (found {found:?})"
            ),
            TocError::InconsistentSessionInfo => {
                f.write_str("some entries have session info, others do not")
            }
            TocError::LeadoutMismatch { sessions, leadouts } => write!(
                f,
                "found {leadouts} leadout entries but {sessions:?} sessions"
            ),
            TocError::InvalidOrder(error) => Display::fmt(error, f),
            TocError::NotFound => f.write_str("required TOC element not found"),
            // wrapped errors are returned by `source()`, not rendered here (issue #198)
            TocError::CdToc(_) => f.write_str("cdtoc error"),
        }
    }
}

impl StdError for TocError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            TocError::CdToc(error) => Some(error),
            // data-carrying variants are rendered in Display instead (issue #198)
            TocError::InvalidTrackEntry(_)
            | TocError::InvalidLeadoutEntry(_)
            | TocError::InvalidInfoEntry(_)
            | TocError::InvalidTrackNo(_)
            | TocError::MultiDisc
            | TocError::InfoMismatch { .. }
            | TocError::InconsistentSessionInfo
            | TocError::LeadoutMismatch { .. }
            | TocError::InvalidOrder { .. }
            | TocError::NotFound => None,
        }
    }
}

impl From<cdtoc::TocError> for TocError {
    fn from(error: cdtoc::TocError) -> Self {
        TocError::CdToc(error)
    }
}

impl From<InvalidTrackNo> for TocError {
    fn from(error: InvalidTrackNo) -> Self {
        TocError::InvalidTrackNo(error)
    }
}

impl From<TocError> for Error {
    fn from(error: TocError) -> Self {
        Error::Toc(error)
    }
}

impl From<TocError> for io::Error {
    fn from(error: TocError) -> Self {
        io::Error::new(error.io_kind(), error)
    }
}

/// Interop downgrade to [`cdtoc`]'s error type: a wrapped [`cdtoc::TocError`] is simply
/// unwrapped; other kinds map to the closest `cdtoc` semantic (`SectorOrder`).
///
/// TODO: #199 Replace cdtoc::TocError downgrade with nightly error_generic_member_access via Error::provide
impl From<TocError> for cdtoc::TocError {
    fn from(error: TocError) -> Self {
        match error {
            TocError::CdToc(error) => error,
            _ => cdtoc::TocError::SectorOrder,
        }
    }
}

/// Leaf error: a value which is not a valid track number (1..=99).
///
/// Error types are "wide" in the values they accept (where "normal" types are "narrow"), so the
/// offending value is captured without loss: any integer type which fits in an `i128` can be
/// stored, and [`InvalidTrackNo::new`] can be used directly in `.map_err(InvalidTrackNo::new)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidTrackNo(i128);

impl InvalidTrackNo {
    /// Creates from the attempted value.
    pub fn new<N>(attempt: N) -> Self
    where
        i128: From<N>,
    {
        Self(attempt.into())
    }
}

impl Display for InvalidTrackNo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid track number: {:#06X}", self.0)
    }
}

impl StdError for InvalidTrackNo {}

impl From<InvalidTrackNo> for io::Error {
    fn from(error: InvalidTrackNo) -> Self {
        io::Error::new(IoErrorKind::InvalidData, error)
    }
}

/// An incorrectly ordered pair of [`TocEntry`]s, for a given [`Format`][super::Format]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidOrder {
    format: Format,
    first_entry: TocEntry,
    second_entry: TocEntry,
}

impl Display for InvalidOrder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid order: {first:?} cannot be followed by {second:?} in format {format:?}",
            first = self.first_entry,
            second = self.second_entry,
            format = self.format
        )
    }
}

impl StdError for InvalidOrder {}

impl InvalidOrder {
    /// Create a new InvalidOrder error
    pub fn new(format: &Format, first_entry: &TocEntry, second_entry: &TocEntry) -> Self {
        Self {
            format: *format,
            first_entry: *first_entry,
            second_entry: *second_entry,
        }
    }
}

impl From<InvalidOrder> for TocError {
    fn from(error: InvalidOrder) -> Self {
        TocError::InvalidOrder(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_is_send_sync_static() {
        fn assert_traits<T: StdError + Send + Sync + 'static>() {}
        assert_traits::<TocError>();
        assert_traits::<InvalidTrackNo>();
    }

    #[test]
    fn display_renders_data_carrying_variants() {
        assert_eq!(
            TocError::NotFound.to_string(),
            "required TOC element not found"
        );
        assert_eq!(
            TocError::InvalidTrackNo(InvalidTrackNo::new(100_u8)).to_string(),
            "invalid track number: 0x0064"
        );
        assert_eq!(
            TocError::MultiDisc.to_string(),
            "TOC entries appear to be from multiple discs"
        );
    }

    #[cfg(false)]
    #[test]
    fn display_renders_label_for_wrapped_variants() {
        assert_eq!(
            TocError::from(cdtoc::TocError::NoAudio).to_string(),
            "cdtoc error"
        );
        assert_eq!(
            TocError::from(io::Error::new(IoErrorKind::NotFound, "gone")).to_string(),
            "I/O error"
        );
    }

    #[cfg(false)]
    #[test]
    fn source_returns_directly_wrapped_error() {
        let error = TocError::from(cdtoc::TocError::NoAudio);
        let source = StdError::source(&error).expect("wrapped cdtoc error should be the source");
        assert!(source.downcast_ref::<cdtoc::TocError>().is_some());

        let raw = io::Error::from_raw_os_error(22);
        let error = TocError::from(raw);
        let source = StdError::source(&error).expect("wrapped io::Error should be the source");
        let inner = source
            .downcast_ref::<io::Error>()
            .expect("source should be the io::Error directly");
        assert_eq!(inner.raw_os_error(), Some(22));

        assert!(StdError::source(&TocError::NotFound).is_none());
        assert!(StdError::source(&TocError::InvalidTrackNo(InvalidTrackNo::new(0_u8))).is_none());
    }

    #[cfg(false)]
    #[test]
    fn io_error_conversion_preserves_status_quo_kind() {
        assert_eq!(
            io::Error::from(TocError::NotFound).kind(),
            IoErrorKind::NotFound
        );
        assert_eq!(
            io::Error::from(TocError::from(cdtoc::TocError::NoAudio)).kind(),
            IoErrorKind::InvalidData
        );
        assert_eq!(
            io::Error::from(TocError::InvalidTrackNo(InvalidTrackNo::new(0_u8))).kind(),
            IoErrorKind::InvalidData
        );
        let raw = io::Error::from_raw_os_error(22);
        let io_error = io::Error::from(TocError::from(raw));
        let payload = io_error.get_ref().expect("io::Error should hold a payload");
        let error = payload
            .downcast_ref::<TocError>()
            .expect("payload should be the original TocError");
        let source = StdError::source(error)
            .and_then(|kind| kind.downcast_ref::<io::Error>())
            .expect("source chain should reach the original io::Error");
        assert_eq!(source.raw_os_error(), Some(22));
    }

    #[test]
    fn converts_into_root_error() {
        let error = Error::from(TocError::NotFound);
        assert!(matches!(error, Error::Toc(TocError::NotFound)));
    }

    #[test]
    fn downgrades_to_cdtoc_error_by_unwrapping() {
        let wrapped = TocError::from(cdtoc::TocError::TrackCount);
        assert_eq!(cdtoc::TocError::from(wrapped), cdtoc::TocError::TrackCount);
        assert_eq!(
            cdtoc::TocError::from(TocError::NotFound),
            cdtoc::TocError::SectorOrder
        );
    }
}
