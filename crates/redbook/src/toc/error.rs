//! Level-2 error for [`toc`][crate::toc]: the [`TocError`] enum & the [`InvalidTrackNo`] leaf.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]: hand-rolled
//! `Error`/`Display`/`source`, `source()` returning the directly wrapped error, and
//! data-carrying variants rendered by `Display`. Conversion to [`io::Error`] is explicit via
//! [`RedbookError::into_io_error`][crate::error::RedbookError::into_io_error], never via
//! `From<_> for io::Error`.
use std::{
    error::Error,
    fmt::{self, Display},
    io,
    num::NonZeroUsize,
};

use crate::{error::RedbookError, toc::Format};

use super::{InfoEntry, RawTocEntry, TocEntry, TrackNo};

/// [`Result`] alias for [`toc`][crate::toc] APIs.
pub type TocResult<T> = Result<T, TocError>;

/// Error returned by [`toc`][crate::toc] APIs: TOC parsing & validation failures.
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
    MultipleDiscs,
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

impl RedbookError for TocError {}

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
            TocError::InvalidTrackNo(_) => write!(f, "invalid track number in TOC"),
            TocError::MultipleDiscs => write!(f, "TOC entries appear to be from multiple discs"),
            TocError::InfoMismatch { entry, found } => write!(
                f,
                "info entry {entry:?} does not match the track entries (found {found:?})"
            ),
            TocError::InconsistentSessionInfo => {
                write!(f, "some entries have session info, others do not")
            }
            TocError::LeadoutMismatch { sessions, leadouts } => write!(
                f,
                "found {leadouts} leadout entries but {sessions:?} sessions"
            ),
            TocError::InvalidOrder(_) => write!(f, "invalid entry order in TOC"),
            TocError::NotFound => write!(f, "required TOC element not found"),
            TocError::CdToc(_) => write!(f, "cdtoc error"),
        }
    }
}

impl Error for TocError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            TocError::CdToc(error) => Some(error),
            TocError::InvalidTrackNo(error) => Some(error),
            TocError::InvalidOrder(error) => Some(error),
            TocError::InvalidTrackEntry(_)
            | TocError::InvalidLeadoutEntry(_)
            | TocError::InvalidInfoEntry(_)
            | TocError::MultipleDiscs
            | TocError::InfoMismatch { .. }
            | TocError::InconsistentSessionInfo
            | TocError::LeadoutMismatch { .. }
            | TocError::NotFound => None,
        }
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            TocError::NotFound => {
                request.provide_value(io::ErrorKind::NotFound);
            }
            TocError::CdToc(_)
            | TocError::InconsistentSessionInfo
            | TocError::InfoMismatch { .. }
            | TocError::InvalidInfoEntry(_)
            | TocError::InvalidLeadoutEntry(_)
            | TocError::InvalidOrder(_)
            | TocError::InvalidTrackEntry(_)
            | TocError::InvalidTrackNo(_)
            | TocError::LeadoutMismatch { .. }
            | TocError::MultipleDiscs => {
                request.provide_value(io::ErrorKind::InvalidData);
            }
        }
    }
}

impl From<cdtoc::TocError> for TocError {
    fn from(error: cdtoc::TocError) -> Self {
        Self::CdToc(error)
    }
}

impl From<InvalidTrackNo> for TocError {
    fn from(error: InvalidTrackNo) -> Self {
        Self::InvalidTrackNo(error)
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

impl Error for InvalidTrackNo {
    // Does not `provide` io::ErrorKind: context dependent InvalidData / InvalidInput
}

impl RedbookError for InvalidTrackNo {}

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

impl Error for InvalidOrder {
    // Does not `provide` io::ErrorKind: context dependent InvalidData / InvalidInput
}

impl RedbookError for InvalidOrder {}

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
        Self::InvalidOrder(error)
    }
}
