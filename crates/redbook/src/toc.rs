//! Support for Tables of Contents
//!
//! A [`TocEntries`] collection is built from [`TocEntry`]s. Each entry is classified from a
//! [`RawTocEntry`] (the raw fields of a Q sub-channel / READ TOC track descriptor) into a
//! [`TrackEntry`], [`LeadoutEntry`] or [`InfoEntry`] where the POINT field is recognised, and
//! otherwise kept as [`TocEntry::Unknown`] raw data.

use std::{
    cmp::Ordering,
    convert::{BoundedCastFromInt, CheckedCastFromInt},
    fmt::Display,
    hint::cold_path,
    marker::StructuralPartialEq,
    num::{NonZeroU8, NonZeroUsize, ParseIntError},
    ops::{Deref, DerefMut},
    slice,
    str::FromStr,
};

use bitflags::bitflags;
use cdtoc::Toc;
use derive_more::Display;
use itertools::{EitherOrBoth, Itertools};
use try_partialord::TrySort;

use crate::{Frame, Track, frame::Msf};

pub use error::{TocError, TocErrorKind};

/// POINT field identifiers used by READ TOC/PMA format 0b0000 (MMC-3, section 5.23.2)
pub mod b0000 {
    use super::Point;
    /// 0b0000 format POINT identifier for Leadout
    pub const LEADOUT_TRACK_ID: Point = Point(0xAA);
}

/// POINT field identifiers used by READ TOC/PMA format 0b0010 (MMC-3, section 5.23.4)
///
/// # Note
/// This is an incomplete set of constants. Only those currently required by the crate are defined.
///
/// This crate specifically aims to support *audio CDs*, additional constants will only be added
/// if required for better audio CD handling.
///
/// # Specification
///
/// The baseline spec is in MMC-3 Table 237
///
/// CTRL | ADR | TNO | POINT | MIN | SEC | FRAME | ZERO | PMIN | PSEC | PFRAME
/// -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | --
/// 4 or 6 | 1 | 00h | 01h–63h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Start position of track | Start position of track | Start position of track
/// 4 or 6 | 1 | 00h | A0h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | First Track Number | Disc Type | 00h
/// 4 or 6 | 1 | 00h | A1h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Last Track Number | 00h | 00h
/// 4 or 6 | 1 | 00h | A2h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Start position of Lead-out | Start position of Lead-out | Start position of Lead-out
/// 4 or 6 | 5 | 00h | B0h | Start time of next possible program in the Recordable Area of the disc | Start time of next possible program in the Recordable Area of the disc | Start time of next possible program in the Recordable Area of the disc | # of pointers in Mode 5 | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc
/// 4 or 6 | 5 | 00h | B1h | 00h | 00h | 00h | 00h | # of skip interval Pointers (N≤40) | # of skip Track Pointers (N≤21) | 00h
/// 4 or 6 | 5 | 00h | B2h–B4h | Skip # | Skip # | Skip # | Skip # | Skip # | Skip # | Skip #
/// 4 or 6 | 5 | 00h | 01h–40h | Ending time for the interval that should be skipped | Ending time for the interval that should be skipped | Ending time for the interval that should be skipped | Reserved | Start time for interval that should be skipped on playback | Start time for interval that should be skipped on playback | Start time for interval that should be skipped on playback
/// 4 or 6 | 5 | 00h | C0h | Optimum recording power | Reserved | Reserved | Reserved | Start time of the first Lead-in Area of the disc | Start time of the first Lead-in Area of the disc | Start time of the first Lead-in Area of the disc
/// 4 or 6 | 5 | 00h | C1h | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP
pub mod b0010 {
    use super::Point;
    /// 0b0010 format POINT identifier for the first track number of a session
    pub const FIRST_TRACK_ID: Point = Point(0xA0);
    /// 0b0010 format POINT identifier for the last track number of a session
    pub const LAST_TRACK_ID: Point = Point(0xA1);
    /// 0b0010 format POINT identifier for Leadout
    pub const LEADOUT_TRACK_ID: Point = Point(0xA2);
}

pub mod error;

/// A raw POINT field value from a TOC track descriptor (Q sub-channel / READ TOC), exactly as
/// read from the disc.
///
/// Any `u8` is a valid `Point`: POINT values include "normal" track numbers (1..=99, see
/// [`TrackNo`]) as well as "special" codes such as the lead-out ([`b0000::LEADOUT_TRACK_ID`],
/// [`b0010::LEADOUT_TRACK_ID`]) or the first/last track info of a session
/// ([`b0010::FIRST_TRACK_ID`], [`b0010::LAST_TRACK_ID`]). Use [`is_normal()`][Self::is_normal],
/// [`is_leadin()`][Self::is_leadin], [`is_info()`][Self::is_info],
/// [`info_kind()`][Self::info_kind] or [`kind()`][Self::kind] for classification.
///
/// # Notes
/// - This is the "lazy" counterpart to the fully-encapsulated [`TrackNo`]: construction is via
///   `Point(n)` / `From<u8>` for simplicity, the field is public and `Deref` & `DerefMut` with
///   `Target = u8` are implemented, so a `Point` can be used interchangeably with a `u8`
/// - Use [`cast()`][Self::cast] rather than `.0.into()` when converting to other integers
/// - `PartialEq` & `PartialOrd` are deliberately not implemented between `Point` & `u8` to ensure
///   comparisons don't inadvertently compare a Point with something semantically different
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct Point(pub u8);

impl Point {
    /// Whether this is a "normal" track number, i.e. convertible to a [`TrackNo`] (1..=99)
    pub const fn is_normal(&self) -> bool {
        TrackNo::new(self.0).is_some()
    }

    /// Whether this is a lead-in/lead-out POINT code: [`b0000::LEADOUT_TRACK_ID`] (0xAA) or
    /// [`b0010::LEADOUT_TRACK_ID`] (0xA2).
    ///
    /// # Notes
    /// - The value depends on the READ TOC format (0b0000 vs 0b0010); the two are deliberately
    ///   not distinguished here. Format 0b0011 (PMA, CD-R/CD-RW only) is a non-goal for this
    ///   audio-focussed crate.
    pub const fn is_leadin(&self) -> bool {
        self.0 == b0000::LEADOUT_TRACK_ID.0 || self.0 == b0010::LEADOUT_TRACK_ID.0
    }

    /// Whether this is a track-info POINT code: [`b0010::FIRST_TRACK_ID`] (0xA0) or
    /// [`b0010::LAST_TRACK_ID`] (0xA1). See [`info_kind()`][Self::info_kind] for which one.
    pub const fn is_info(&self) -> bool {
        self.info_kind().is_some()
    }

    /// The track-info kind of this POINT: `Some(First)` for [`b0010::FIRST_TRACK_ID`],
    /// `Some(Last)` for [`b0010::LAST_TRACK_ID`], `None` for any other value
    pub const fn info_kind(&self) -> Option<FirstLast> {
        match self.0 {
            p if p == b0010::FIRST_TRACK_ID.0 => Some(FirstLast::First),
            p if p == b0010::LAST_TRACK_ID.0 => Some(FirstLast::Last),
            _ => None,
        }
    }

    /// The kind of this POINT value
    pub const fn kind(&self) -> PointKind {
        use PointKind::*;

        match self.0 {
            _ if self.is_leadin() => LeadIn,
            p if p == b0010::FIRST_TRACK_ID.0 => FirstTrackInfo,
            p if p == b0010::LAST_TRACK_ID.0 => LastTrackInfo,
            _ if self.is_normal() => Normal,
            _ => Unknown,
        }
    }

    /// Cast to another integer type (except i8)
    pub const fn cast<N>(self) -> N
    where
        N: [const] From<u8>,
    {
        N::from(self.0)
    }
}

// ==========================================
// Point - const comparison & conversion ops
// ==========================================

const impl Ord for Point {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

const impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const impl Eq for Point {}

const impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl StructuralPartialEq for Point {}

impl Deref for Point {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Point {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for Point {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "point: {n:#04X}", n = self.0)
    }
}

const impl From<u8> for Point {
    fn from(n: u8) -> Self {
        Point(n)
    }
}

impl FromStr for Point {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
}

/// The kind of a raw [`Point`] value in a TOC track descriptor.
///
/// Returned by [`Point::kind`]. Extensible in future, e.g. with ADR 5 (recordable disc)
/// related values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Display)]
pub enum PointKind {
    /// A "normal" track number, i.e. 1..=[`TrackNo::MAX`]
    Normal,
    /// The lead-in/lead-out POINT code: [`b0000::LEADOUT_TRACK_ID`] (0xAA) or
    /// [`b0010::LEADOUT_TRACK_ID`] (0xA2), not distinguished
    LeadIn,
    /// The first track number of a session: [`b0010::FIRST_TRACK_ID`] (0xA0)
    FirstTrackInfo,
    /// The last track number of a session: [`b0010::LAST_TRACK_ID`] (0xA1)
    LastTrackInfo,
    /// Any other, unrecognised POINT value
    Unknown,
}

/// A valid track number: always within 1..=[`TrackNo::MAX`] (1..=99).
///
/// # Notes
/// - Track numbers are stored on physical discs in a single byte; redbook defines the valid
///   range as 1..=99
/// - The invariant is upheld by the type itself: the field is private and stores a
///   [`NonZeroU8`] (so 0 is unrepresentable), and every constructor validates against
///   [`TrackNo::MAX`]. "Special" POINT values (0xA0..=0xA2, 0xAA, ...) are not track numbers:
///   they are [`Point`]s and cannot be constructed as a `TrackNo` at all
/// - Encapsulation is consistent with [`Frame`] & [`frame::ByteSize`]: `Deref` & `DerefMut` are
///   deliberately not implemented; use [`cast()`][Self::cast] & friends to convert to a
///   primitive integer
///
/// # Construction & conversion
/// - [`new()`][Self::new] (validating, returns `Option`), [`strict_new()`][Self::strict_new]
///   (panicking) and [`saturating_new()`][Self::saturating_new] are generic over any integer
///   type, as are [`cast()`][Self::cast], [`checked_cast()`][Self::checked_cast],
///   [`saturating_cast()`][Self::saturating_cast] & [`strict_cast()`][Self::strict_cast]
///
/// # Arithmetic operations
/// - `+`, `-`, etc. are deliberately not implemented for `TrackNo`. If arithmetic is needed,
///   `cast()` to a primitive integer first and construct the result via `new()` & friends.
///
/// # Equality & Ordering
/// - `PartialEq` & `PartialOrd` are deliberately not implemented between `TrackNo` & `u8` to ensure
///   comparisons don't inadvertently compare a TrackNo with something semantically different
///
/// # Pattern matching
/// - [`TrackNo`] implements [`StructuralPartialEq`], so constants such as [`TrackNo::MAX`] can
///   be used directly as `match` arms
///
/// # Examples
/// ```
/// use redbook::TrackNo;
///
/// assert_eq!(TrackNo::new(7_u16), Some(TrackNo::strict_new(7)));
/// assert_eq!(TrackNo::new(0_u8), None);
/// assert_eq!(TrackNo::new(100_u8), None);
/// assert_eq!(TrackNo::new(256_u16), None);
/// assert_eq!(TrackNo::MAX.cast::<u8>(), 99);
/// ```
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash)]
pub struct TrackNo(NonZeroU8);

impl TrackNo {
    /// The maximum track number: track 99
    ///
    /// Redbook defines the valid range for track numbers as 1..=99
    pub const MAX: Self = Self(NonZeroU8::new(99).unwrap());

    /// Creates a new `TrackNo` from any integer type, returning `None` if the value does not
    /// fit in a `u8` (track numbers are stored on disc in a single byte) or is outside
    /// 1..=[`Self::MAX`].
    ///
    /// # Note
    /// - For a panicking constructor use [`Self::strict_new`] directly, not `Self::new().unwrap()`
    /// - For a saturating constructor use [`Self::saturating_new`]
    pub const fn new<N>(value: N) -> Option<Self>
    where
        u8: [const] CheckedCastFromInt<N>,
    {
        Self::from_u8(u8::checked_cast_from(value)?).and_then(Self::validate)
    }

    /// Creates a new `TrackNo` from any integer type, panicking if the value is not a valid
    /// track number.
    ///
    /// # Note
    /// - For a non-panicking constructor use [`Self::new`] or [`Self::saturating_new`]
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the value is 0 or above [`Self::MAX`].
    pub const fn strict_new<N>(value: N) -> Self
    where
        u8: [const] CheckedCastFromInt<N>,
    {
        match Self::new(value) {
            Some(track_no) => track_no,
            None => panic!("invalid track number: must be within 1..=99"),
        }
    }

    /// Creates a new `TrackNo` from any integer type, saturating at the boundaries of the valid
    /// range: values below 1 become track 1, values above [`Self::MAX`] become [`Self::MAX`].
    ///
    /// # Note
    /// - For a validating constructor use [`Self::new`] or for a panicking constructor
    ///   use [`Self::strict_new`]
    pub const fn saturating_new<N>(value: N) -> Self
    where
        u8: [const] BoundedCastFromInt<N>,
    {
        let inner = u8::saturating_cast_from(value)
            .max(1)
            .min(Self::MAX.0.get());
        // Cannot panic: inner is clamped into 1..=99
        Self(NonZeroU8::new(inner).unwrap())
    }

    /// Returns the underlying `u8` representation.
    ///
    /// For clarity it is recommended to use [`Self::cast`] even when casting to `u8`.
    const fn into_inner(self) -> u8 {
        self.0.get()
    }

    /// Constructs from a `u8`, returning `None` for 0 (track numbers are non-zero).
    const fn from_u8(inner: u8) -> Option<Self> {
        NonZeroU8::new(inner).map(Self)
    }

    /// Casts to any integer type which the inner representation allows without risk.
    pub const fn cast<N>(self) -> N
    where
        N: [const] From<u8>,
    {
        N::from(self.into_inner())
    }

    /// Casts to any integer type returning `None` on overflow.
    pub const fn checked_cast<N>(self) -> Option<N>
    where
        N: [const] CheckedCastFromInt<u8>,
    {
        N::checked_cast_from(self.into_inner())
    }

    /// Casts to any integer type saturating on overflow.
    pub const fn saturating_cast<N>(self) -> N
    where
        N: [const] BoundedCastFromInt<u8>,
    {
        N::saturating_cast_from(self.into_inner())
    }

    /// Casts to any integer type panicking on overflow.
    pub const fn strict_cast<N>(self) -> N
    where
        N: [const] CheckedCastFromInt<u8>,
    {
        N::strict_cast_from(self.into_inner())
    }

    /// Validates `self` is within the valid range (always true: the invariant is upheld by the
    /// private [`NonZeroU8`] field & the validating constructors)
    pub const fn is_valid(&self) -> bool {
        self.0.get() <= Self::MAX.0.get()
    }

    /// Validates `self` and returns `None` if invalid
    pub const fn validate(self) -> Option<Self> {
        self.is_valid().then_some(self)
    }

    /// Validates `self`, panicking if invalid
    ///
    /// # Panics
    /// Panics if `self` is above [`Self::MAX`].
    pub const fn assert_valid(self) -> Self {
        assert!(self.is_valid(), "semantic overflow");
        self
    }

    /// Replaces invalid values with [`Self::MAX`]. No need to saturate at 0 as this is covered
    /// by the internal representation.
    pub const fn saturate(self) -> Self {
        match self.is_valid() {
            true => self,
            false => Self::MAX,
        }
    }
}

// ==========================================
// TrackNo - const comparison & conversion ops
// ==========================================

const impl Ord for TrackNo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.into_inner().cmp(&other.into_inner())
    }
}

const impl PartialOrd for TrackNo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const impl Eq for TrackNo {}

const impl PartialEq for TrackNo {
    fn eq(&self, other: &Self) -> bool {
        self.into_inner() == other.into_inner()
    }
}

impl StructuralPartialEq for TrackNo {}

impl Display for TrackNo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "track: {n}", n = self.0)
    }
}

/// Entry in a CD TOC (Table of Contents) for an audio track.
///
/// Represents a single entry from the CD's Table of Contents, containing the
/// track number and its absolute start position on the disc.
///
/// # Notes
///
/// - The start position includes the lead-in area (150 frames)
/// - As this library is focussed on audio (redbook), only data relevant to audio tracks is
///   maintained. Conversion functions should be designed discard or fail on data tracks.
///
/// # Examples
///
/// ```
/// use std::num::NonZeroU8;
///
/// use redbook::{Frame, LEADIN};
/// use redbook::toc::{Control, Point, TrackEntry, TrackNo, b0010};
///
/// let entry = TrackEntry::new(
///     TrackNo::strict_new(1),
///     LEADIN,
///     Control::default(),
///     Some(NonZeroU8::MIN),
/// );
/// assert_eq!(entry.trackno(), TrackNo::strict_new(1));
/// assert_eq!(entry.start(), LEADIN);
/// assert!(entry.is_audio_track());
///
/// // "special" POINT values (e.g. 0xA0..=0xA2, 0xAA) are `Point`s, not track numbers:
/// // they cannot be passed to `TrackEntry::new` at all
/// assert!(!b0010::LEADOUT_TRACK_ID.is_normal());
/// assert!(b0010::LEADOUT_TRACK_ID.is_leadin());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackEntry {
    /// The 1-indexed track number.
    track: TrackNo,
    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    start: Frame,
    /// The control code, identifying track type
    control: Control,
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed)
    session: Option<NonZeroU8>,
}

impl TrackEntry {
    /// Construct a new [`TrackEntry`].
    ///
    /// # Notes
    /// - `track` needs no validation: a [`TrackNo`] is always a valid track number (1..=99).
    ///   "Special" POINT values (e.g. 0xA0..=0xA2, 0xAA) are [`Point`]s and belong in the other
    ///   [`TocEntry`] variants.
    /// - `start` is deliberately *not* validated against the lead-in: copy-protected discs
    ///   (e.g. Cactus Data Shield, Key2Audio) deliberately store non-standard track positions,
    ///   and input from any reasonable source must be handled without failing.
    /// - session validity is enforced at compile time: `session` is an `Option<NonZeroU8>`
    pub fn new(track: TrackNo, start: Frame, control: Control, session: Option<NonZeroU8>) -> Self {
        Self {
            track,
            start,
            control,
            session,
        }
    }

    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    pub fn start(&self) -> Frame {
        self.start
    }

    /// The 1-indexed track number.
    pub fn trackno(&self) -> TrackNo {
        self.track
    }

    /// The session this entry relates to, if known.
    pub fn session(&self) -> Option<NonZeroU8> {
        self.session
    }

    /// Is an audio track (based on control code & track number)
    pub fn is_audio_track(&self) -> bool {
        self.is_audio()
    }

    /// Is an data track (based on control code & track number)
    pub fn is_data_track(&self) -> bool {
        self.is_data()
    }
}

impl Ctrl for TrackEntry {
    fn control(&self) -> Control {
        self.control
    }
}

impl PartialOrd for TrackEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use Ordering::*;

        (self.session.is_some() == other.session.is_some()).then_some(())?;

        match self.session.cmp(&other.session) {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ if self.track.cmp(&other.track) == self.start.cmp(&other.start) => {
                Some(self.track.cmp(&other.track))
            }
            _ => None,
        }
    }
}

/// Entry in a CD TOC (Table of Contents) for the lead-out.
///
/// The lead-out marks the end of a session. It has no track number, only an absolute start
/// position on the disc (including the 150-frame lead-in).
///
/// # Notes
/// - Construct via [`LeadoutEntry::new`]. The start position is deliberately *not* validated
///   against the lead-in: copy-protected discs store non-standard positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LeadoutEntry {
    /// Absolute start position of the lead-out on the disc, including lead-in (150 frames).
    start: Frame,
    /// The control code, identifying track type
    control: Control,
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed)
    session: Option<NonZeroU8>,
}

impl LeadoutEntry {
    /// Construct a new [`LeadoutEntry`].
    ///
    /// # Notes
    /// - `start` is deliberately *not* validated against the lead-in, for the same reason as
    ///   [`TrackEntry::new`]: copy-protected discs store non-standard positions.
    /// - session validity is enforced at compile time: `session` is an `Option<NonZeroU8>`.
    pub fn new(start: Frame, control: Control, session: Option<NonZeroU8>) -> Self {
        Self {
            start,
            control,
            session,
        }
    }

    /// Absolute start position of the lead-out on the disc, including lead-in (150 frames).
    pub fn start(&self) -> Frame {
        self.start
    }

    /// The session this entry relates to, if known.
    pub fn session(&self) -> Option<NonZeroU8> {
        self.session
    }
}

/// Partial Ordering based on session then start. If these match but control differs then
/// no meaningful ordering is possible
impl PartialOrd for LeadoutEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use Ordering::*;

        (self.session.is_some() == other.session.is_some()).then_some(())?;

        match self
            .session
            .cmp(&other.session)
            .then_with(|| self.start.cmp(&other.start))
        {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => None,
        }
    }
}

impl Ctrl for LeadoutEntry {
    fn control(&self) -> Control {
        self.control
    }
}

/// Entry in a CD TOC (Table of Contents) holding session info: the first or last track number
/// of a session.
///
/// Encoded as POINT 0xA0 ([`b0010::FIRST_TRACK_ID`]) / 0xA1 ([`b0010::LAST_TRACK_ID`]) in READ
/// TOC format 0b0010, with the track number stored in the start minute (PMIN) of the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InfoEntry {
    /// The session this entry relates to (`NonZeroU8`: session numbers are 1-indexed).
    session: NonZeroU8,
    /// The first or last track number of the session, as encoded in the start minute.
    track: TrackNo,
    /// The control code, identifying track type
    control: Control,
    /// Whether this holds the first or the last track number of the session.
    info_type: FirstLast,
}

impl InfoEntry {
    /// Construct a new [`InfoEntry`] (first/last track number info).
    ///
    /// # Notes
    /// - `track` needs no validation: a [`TrackNo`] is always a valid track number (1..=99)
    /// - session validity is enforced at compile time: `session` is a [`NonZeroU8`]
    pub fn new(session: NonZeroU8, track: TrackNo, control: Control, info_type: FirstLast) -> Self {
        Self {
            session,
            track,
            control,
            info_type,
        }
    }

    /// The session this entry relates to.
    pub fn session(&self) -> NonZeroU8 {
        self.session
    }

    /// The first or last track number of the session.
    pub fn trackno(&self) -> TrackNo {
        self.track
    }

    /// Whether this holds the first or the last track number of the session.
    pub fn info_type(&self) -> FirstLast {
        self.info_type
    }
}

impl Ctrl for InfoEntry {
    fn control(&self) -> Control {
        self.control
    }
}

/// Whether an [`InfoEntry`] holds the first or the last track number of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FirstLast {
    /// The first track number of the session (POINT [`b0010::FIRST_TRACK_ID`])
    First,
    /// The last track number of the session (POINT [`b0010::LAST_TRACK_ID`])
    Last,
}

/// Partial ordering based on session then type. If these match but other fields differ then
/// no meaningful ordering is possible
impl PartialOrd for InfoEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use Ordering::*;

        match self
            .session
            .cmp(&other.session)
            .then_with(|| self.info_type.cmp(&other.info_type))
        {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => None,
        }
    }
}

/// A raw, not-yet-classified TOC entry: the fields of a Q sub-channel / READ TOC track
/// descriptor, exactly as read from the disc.
///
/// Classify into a higher-level [`TocEntry`] via `Option::<TocEntry>::from(raw)` — `None` means
/// the entry is recognizably a track / lead-out / track-info entry but invalid.
///
/// For finer-grained control use [`RawTocEntry::kind`] to determine the entry kind, then the
/// individual `TryFrom<RawTocEntry>` impls ([`TrackEntry`], [`LeadoutEntry`], [`InfoEntry`]) to
/// extract the classified payload.
///
/// The relevant definitions are:
///
/// #### READ_TOC format 0010b (detailled TOC, multi-session)
///
/// CTRL   ADR | TNO | POINT | MIN | SEC | FRAME | ZERO | PMIN | PSEC | PFRAME
/// -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | --
/// 4 or 6 | 1 | 00h | 01h–63h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Start position of track | Start position of track | Start position of track
/// 4 or 6 | 1 | 00h | A0h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | First Track Number | Disc Type | 00h
/// 4 or 6 | 1 | 00h | A1h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Last Track Number | 00h | 00h
/// 4 or 6 | 1 | 00h | A2h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Start position of Lead-out | Start position of Lead-out | Start position of Lead-out
/// 4 or 6 | 5 | 00h | B0h | Start time of next possible program in the Recordable Area of the disc | Start time of next possible program in the Recordable Area of the disc | Start time of next possible program in the Recordable Area of the disc | # of pointers in Mode 5 | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc
/// 4 or 6 | 5 | 00h | B1h | 00h | 00h | 00h | 00h | # of skip interval Pointers (N≤40) | # of skip Track Pointers (N≤21) | 00h
/// 4 or 6 | 5 | 00h | B2h–B4h | Skip # | Skip # | Skip # | Skip # | Skip # | Skip # | Skip #
/// 4 or 6 | 5 | 00h | 01h–40h | Ending time for the interval that should be skipped | Ending time for the interval that should be skipped | Ending time for the interval that should be skipped | Reserved | Start time for interval that should be skipped on playback | Start time for interval that should be skipped on playback | Start time for interval that should be skipped on playback
/// 4 or 6 | 5 | 00h | C0h | Optimum recording power | Reserved | Reserved | Reserved | Start time of the first Lead-in Area of the disc | Start time of the first Lead-in Area of the disc | Start time of the first Lead-in Area of the disc
/// 4 or 6 | 5 | 00h | C1h | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawTocEntry {
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed)
    session: Option<NonZeroU8>,
    /// The ADR (address mode) field of the Q sub-channel.
    adr: u8,
    /// The control code, identifying track type
    control: Control,
    /// The track number this entry relates to (TNO field).
    tno: u8,
    /// The POINT field: a track number or a special code (e.g. 0xA0..=0xA2, 0xAA).
    point: Point,
    /// The MSF (min, sec, frame) field.
    msf: Msf,
    /// The "zero" byte of the track descriptor.
    zero: u8,
    /// The PMSF (point min, sec, frame) field: the payload data for POINT.
    pmsf: Msf,
}

impl RawTocEntry {
    /// Construct a new [`RawTocEntry`] from raw Q sub-channel / READ TOC data.
    ///
    /// As this is a raw, not-yet-classified entry there is nothing to validate; session validity
    /// is enforced at compile time via `Option<NonZeroU8>`.
    #[expect(
        clippy::too_many_arguments,
        reason = "arguments mirror the fields of a raw READ TOC track descriptor one-to-one"
    )]
    pub fn new(
        session: Option<NonZeroU8>,
        adr: u8,
        control: Control,
        tno: u8,
        point: Point,
        msf: Msf,
        zero: u8,
        pmsf: Msf,
    ) -> Self {
        Self {
            session,
            adr,
            control,
            tno,
            point,
            msf,
            zero,
            pmsf,
        }
    }

    /// The session this entry relates to, if known.
    pub fn session(&self) -> Option<NonZeroU8> {
        self.session
    }

    /// The ADR (address mode) field of the Q sub-channel.
    pub fn adr(&self) -> u8 {
        self.adr
    }

    /// The track number this entry relates to (TNO field).
    pub fn tno(&self) -> u8 {
        self.tno
    }

    /// The POINT field: a track number or a special code (e.g. 0xA0..=0xA2, 0xAA).
    pub fn point(&self) -> Point {
        self.point
    }

    /// The MSF (min, sec, frame) field.
    pub fn msf(&self) -> Msf {
        self.msf
    }

    /// The "zero" byte of the track descriptor.
    pub fn zero(&self) -> u8 {
        self.zero
    }

    /// The PMSF (point min, sec, frame) field: the payload data for POINT.
    pub fn pmsf(&self) -> Msf {
        self.pmsf
    }

    /// Classify this raw entry by its ADR / POINT fields (and control code for the audio/data
    /// distinction), without validating the payload data.
    ///
    /// This is the same classification that [`From<RawTocEntry> for Option<TocEntry>`] applies,
    /// exposed separately so callers can determine which [`TryFrom<RawTocEntry>`] impl to use
    /// before attempting the conversion.
    pub fn kind(&self) -> TocEntryKind {
        use TocEntryKind::*;
        use TrackKind::*;

        match (self.adr, self.point.kind(), self.control().kind()) {
            (1, PointKind::LeadIn, Audio) => AudioLeadout,
            (1, PointKind::LeadIn, Data) => DataLeadout,
            (1, PointKind::FirstTrackInfo, Audio) => FirstAudioTrackInfo,
            (1, PointKind::FirstTrackInfo, Data) => FirstDataTrackInfo,
            (1, PointKind::LastTrackInfo, Audio) => LastAudioTrackInfo,
            (1, PointKind::LastTrackInfo, Data) => LastDataTrackInfo,
            (1, PointKind::Normal, Audio) => AudioTrack,
            (1, PointKind::Normal, Data) => DataTrack,
            _ => Unknown,
        }
    }
}

impl Ctrl for RawTocEntry {
    fn control(&self) -> Control {
        self.control
    }
}

impl PartialOrd for RawTocEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use std::cmp::Ordering::*;

        (self.session.is_some() == other.session.is_some()).then_some(())?;

        match self.session.cmp(&other.session) {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => {
                // destructure to ensure we don't forget any fields
                let RawTocEntry {
                    session: _,
                    adr,
                    control,
                    tno,
                    point,
                    msf,
                    zero,
                    pmsf,
                } = self;

                // everything except point must match to be able to order
                (*adr == other.adr
                    && *control == other.control
                    && *tno == other.tno
                    && *msf == other.msf
                    && *zero == other.zero
                    && *pmsf == other.pmsf)
                    .then(|| point.cmp(&other.point))
            }
        }
    }
}

/// A single, classified entry from a CD's Table of Contents (TOC).
///
/// Entries are classified from [`RawTocEntry`]s by their POINT field: a normal track number
/// becomes a [`Track`][TocEntry::Track] entry, [`b0010::FIRST_TRACK_ID`] /
/// [`b0010::LAST_TRACK_ID`] become [`TrackInfo`][TocEntry::TrackInfo] entries, a lead-out point
/// ([`b0000::LEADOUT_TRACK_ID`] or [`b0010::LEADOUT_TRACK_ID`]) becomes a
/// [`Leadout`][TocEntry::Leadout] entry, and anything else is kept as [`Unknown`][TocEntry::Unknown]
/// raw data.
///
/// Classification is validating: `Option::<TocEntry>::from(raw)` returns `None` when an entry is
/// recognizably a track / lead-out / track-info entry but invalid (e.g. an A0/A1 point without
/// session info, or a non-normal track number in PMIN), rather than classifying it as `Unknown`.
///
/// The classification logic is split into [`RawTocEntry::kind`] (which determines the entry
/// kind from ADR/POINT/control) and individual `TryFrom<RawTocEntry>` impls for
/// [`TrackEntry`], [`LeadoutEntry`] and [`InfoEntry`]; `From<RawTocEntry> for Option<TocEntry>`
/// glues these together.
///
/// # Notes
/// - As this library is focussed on audio (redbook), only data relevant to audio tracks is
///   modelled. Use the [`Ctrl`] trait (e.g. [`.control().kind()`][Ctrl::kind]) to distinguish audio
///   from data entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TocEntry {
    /// An audio or data track - data parsed from POINT & PMin,PSec,PFrame
    Track(TrackEntry),
    /// A leadout - based on POINT
    Leadout(LeadoutEntry),
    /// First/Last track number - based on POINT, data parsed from PMin
    TrackInfo(InfoEntry),
    /// Something else - raw data
    Unknown(RawTocEntry),
}

impl From<TrackEntry> for TocEntry {
    fn from(entry: TrackEntry) -> Self {
        Self::Track(entry)
    }
}

impl From<LeadoutEntry> for TocEntry {
    fn from(entry: LeadoutEntry) -> Self {
        Self::Leadout(entry)
    }
}

impl From<InfoEntry> for TocEntry {
    fn from(entry: InfoEntry) -> Self {
        Self::TrackInfo(entry)
    }
}

/// Attempts to classify a raw entry as a [`TrackEntry`].
///
/// Fails when the entry is not a track entry (ADR != 1 or POINT is not a normal track number),
/// or when the payload is invalid. Use [`RawTocEntry::kind`] if you need to check applicability
/// first. Or use `From<RawTocEntry> for Option<TocEntry>` directly to identify, validate and
/// convert in one step.
impl TryFrom<RawTocEntry> for TrackEntry {
    type Error = TocError;

    fn try_from(raw: RawTocEntry) -> Result<Self, Self::Error> {
        match (raw.adr(), raw.point()) {
            // TODO: #194 Make use of error-interconversion
            (1, point) if point.is_normal() => {
                let track = TrackNo::new(point.0)
                    .ok_or_else(|| TocError::invalid_data().context("invalid track entry"))?;
                Ok(TrackEntry::new(
                    track,
                    raw.pmsf().into(),
                    raw.control(),
                    raw.session(),
                ))
            }
            _ => Err(TocError::invalid_data().context(format!(
                "not a track entry: adr={}, point={:?}",
                raw.adr(),
                raw.point()
            ))),
        }
    }
}

/// Attempts to classify a raw entry as a [`LeadoutEntry`].
///
/// Fails when the entry is not a lead-out entry (ADR ≠ 1 or POINT is not a lead-out code).
/// Use [`RawTocEntry::kind`] to check applicability first.
impl TryFrom<RawTocEntry> for LeadoutEntry {
    type Error = TocError;

    fn try_from(raw: RawTocEntry) -> Result<Self, Self::Error> {
        match (raw.adr(), raw.point()) {
            (1, point) if point.is_leadin() => Ok(LeadoutEntry::new(
                raw.pmsf().into(),
                raw.control(),
                raw.session(),
            )),
            _ => Err(TocError::invalid_data().context(format!(
                "not a leadout entry: adr={}, point={:?}",
                raw.adr(),
                raw.point()
            ))),
        }
    }
}

/// Attempts to classify a raw entry as an [`InfoEntry`] (first/last track number of a session).
///
/// Fails when the entry is not an info entry (ADR ≠ 1 or POINT is not 0xA0/0xA1), when session
/// info is missing, or when the PMIN payload is not a normal track number.
/// Use [`RawTocEntry::kind`] to check applicability first.
impl TryFrom<RawTocEntry> for InfoEntry {
    type Error = TocError;

    fn try_from(raw: RawTocEntry) -> Result<Self, Self::Error> {
        let info_type = match (raw.adr(), raw.point().info_kind()) {
            (1, Some(info_type)) => info_type,
            _ => {
                return Err(TocError::invalid_data().context(format!(
                    "not an info entry: adr={}, point={:?}",
                    raw.adr(),
                    raw.point()
                )));
            }
        };
        let session = raw
            .session()
            .ok_or_else(|| TocError::invalid_data().context("info entry requires session info"))?;
        let track = TrackNo::new(raw.pmsf().as_u8s().0).ok_or_else(|| {
            TocError::invalid_data().context("invalid info entry: non-normal track number in PMIN")
        })?;
        Ok(InfoEntry::new(session, track, raw.control(), info_type))
    }
}

/// Classifies a raw entry into a higher-level [`TocEntry`], returning `None` when the entry is
/// recognizably a track / lead-out / track-info entry but invalid (e.g. an A0/A1 point without
/// session info, or with a non-normal track number in PMIN).
///
/// This is the glue over [`RawTocEntry::kind`] and the individual
/// [`TryFrom<RawTocEntry>`][TrackEntry#impl-TryFrom<RawTocEntry>-for-TrackEntry] impls:
/// `kind()` decides which conversion to attempt, and a failed `TryFrom` becomes `None`.
///
/// [`TocEntry::Unknown`] is reserved for valid entries which the higher-level abstractions don't
/// model (e.g. session boundary or writing-speed details).
impl From<RawTocEntry> for Option<TocEntry> {
    fn from(raw: RawTocEntry) -> Self {
        use TocEntryKind::*;

        match raw.kind() {
            AudioTrack | DataTrack => TrackEntry::try_from(raw).ok().map(TocEntry::Track),
            AudioLeadout | DataLeadout => LeadoutEntry::try_from(raw).ok().map(TocEntry::Leadout),
            FirstAudioTrackInfo | FirstDataTrackInfo | LastAudioTrackInfo | LastDataTrackInfo => {
                InfoEntry::try_from(raw).ok().map(TocEntry::TrackInfo)
            }
            Unknown => Some(TocEntry::Unknown(raw)),
            Zero => None,
        }
    }
}

/// Partial ordering *per session* is based upon logical order on the physical disc as follows:
/// - FirstTrackInfo (one per session)
/// - LastTrackInfo (one per session)
/// - Tracks
/// - Leadout (one per session)
/// - Any additional raw entries
/// - ... next session ...
///
/// If entries are mixed from different discs then meaningful ordering may be impossible
impl PartialOrd for TocEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use TocEntry::*;
        use std::cmp::Ordering::*;

        match self.session().cmp(&other.session()) {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => match (self, other) {
                (TrackInfo(i), TrackInfo(o)) => i.partial_cmp(o),
                (TrackInfo(_), _) => Some(Less),
                (_, TrackInfo(_)) => Some(Greater),
                (Track(t), Track(o)) => t.partial_cmp(o),
                (Track(_), _) => Some(Less),
                (_, Track(_)) => Some(Greater),
                (Leadout(l), Leadout(o)) => l.partial_cmp(o),
                (Leadout(_), _) => Some(Less),
                (_, Leadout(_)) => Some(Greater),
                (Unknown(raw), Unknown(o)) => raw.partial_cmp(o),
            },
        }
    }
}

impl Ctrl for TocEntry {
    fn control(&self) -> Control {
        match self {
            TocEntry::Track(entry) => entry.control(),
            TocEntry::Leadout(entry) => entry.control(),
            TocEntry::TrackInfo(entry) => entry.control(),
            TocEntry::Unknown(entry) => entry.control(),
        }
    }
}

impl TocEntry {
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed).
    pub fn session(&self) -> Option<NonZeroU8> {
        match self {
            TocEntry::Track(entry) => entry.session(),
            TocEntry::Leadout(entry) => entry.session(),
            TocEntry::TrackInfo(entry) => Some(entry.session()),
            TocEntry::Unknown(entry) => entry.session(),
        }
    }

    /// The track number this entry relates to, if it has one.
    ///
    /// `Some` for [`Track`][TocEntry::Track] and [`TrackInfo`][TocEntry::TrackInfo] entries;
    /// `None` for [`Leadout`][TocEntry::Leadout] and [`Unknown`][TocEntry::Unknown] entries.
    pub fn trackno(&self) -> Option<TrackNo> {
        match self {
            TocEntry::Track(entry) => Some(entry.trackno()),
            TocEntry::Leadout(_) => None,
            TocEntry::TrackInfo(entry) => Some(entry.trackno()),
            TocEntry::Unknown(_) => None,
        }
    }

    /// Whether this is an audio [`Track`][TocEntry::Track] entry (control code & track number).
    pub fn is_audio_track(&self) -> bool {
        self.is_track() && self.is_audio()
    }

    /// Whether this is a data [`Track`][TocEntry::Track] entry (control code & track number).
    pub fn is_data_track(&self) -> bool {
        self.is_track() && self.is_data()
    }

    /// Whether this is a [`Track`][TocEntry::Track] entry.
    pub fn is_track(&self) -> bool {
        matches!(self, TocEntry::Track(_))
    }

    /// Whether this is a [`Leadout`][TocEntry::Leadout] entry.
    pub fn is_leadout(&self) -> bool {
        matches!(self, TocEntry::Leadout(_))
    }

    /// The [`LeadoutEntry`] payload, if this is a [`Leadout`][TocEntry::Leadout] entry.
    pub fn leadoutentry(&self) -> Option<&LeadoutEntry> {
        match self {
            TocEntry::Leadout(leadout) => Some(leadout),
            _ => None,
        }
    }

    /// The [`TrackEntry`] payload, if this is a [`Track`][TocEntry::Track] entry.
    pub fn trackentry(&self) -> Option<&TrackEntry> {
        match self {
            TocEntry::Track(entry) => Some(entry),
            _ => None,
        }
    }

    /// The [`InfoEntry`] payload, if this is a [`TrackInfo`][TocEntry::TrackInfo] entry.
    pub fn infoentry(&self) -> Option<&InfoEntry> {
        match self {
            TocEntry::TrackInfo(entry) => Some(entry),
            _ => None,
        }
    }

    /// The absolute start position on the disc (including the 150-frame lead-in), if this
    /// entry has one: [`Track`][TocEntry::Track] and [`Leadout`][TocEntry::Leadout] entries do.
    pub fn start(&self) -> Option<Frame> {
        match self {
            TocEntry::Track(entry) => Some(entry.start()),
            TocEntry::Leadout(entry) => Some(entry.start()),
            TocEntry::TrackInfo(_) => None,
            TocEntry::Unknown(_) => None,
        }
    }

    /// The kind of entry, combining the variant with the control code (audio vs data).
    ///
    /// Useful for cases where you need to pattern match on the detailed type without
    /// deconstructing the related entry data.
    pub fn kind(&self) -> TocEntryKind {
        use FirstLast::*;
        use TocEntry::*;
        use TocEntryKind::*;
        use TrackKind::*;

        match (self, self.control().kind()) {
            (Track(_), Audio) => AudioTrack,
            (Track(_), Data) => DataTrack,
            (Leadout(_), Audio) => AudioLeadout,
            (Leadout(_), Data) => DataLeadout,
            (
                TrackInfo(InfoEntry {
                    info_type: First, ..
                }),
                Audio,
            ) => FirstAudioTrackInfo,
            (
                TrackInfo(InfoEntry {
                    info_type: First, ..
                }),
                Data,
            ) => FirstDataTrackInfo,
            (
                TrackInfo(InfoEntry {
                    info_type: Last, ..
                }),
                Audio,
            ) => LastAudioTrackInfo,
            (
                TrackInfo(InfoEntry {
                    info_type: Last, ..
                }),
                Data,
            ) => LastDataTrackInfo,
            (TocEntry::Unknown(_), _) => TocEntryKind::Unknown,
        }
    }
}

impl Display for TrackEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{n}, start: {start}, control codes: {ctl}",
            n = self.track,
            start = self.start,
            ctl = self.control
        )
    }
}

impl PartialEq<cdtoc::Track> for TrackEntry {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (Some(self.track) == TrackNo::new(toc_track.number()))
            && Msf::try_from(toc_track.msf()).is_ok_and(|msf| self.start == msf)
    }
}

/// Q Sub-Channel control flags, as per table 222 of MMC-3 draft (section 5.22.2)
///
/// # Interpretation
/// | Value |  | Definition |
/// |------:|--|:-----------|
/// | 00x0b |  | 2 audio channels without pre-emphasis |
/// | 00x1b |  | 2 audio channels with pre-emphasis of 50/15 µs |
/// | 10x0b |  | audio channels without pre-emphasis (reserved in CD-R/RW) |
/// | 10x1b |  | audio channels with pre-emphasis of 50/15 µs (reserved in CD-R/RW) |
/// | 01x0b |  | Data track, recorded uninterrupted |
/// | 01x1b |  | Data track, recorded incremental |
/// | 11xxb |  | Reserved |
/// | xx0xb |  | digital copy prohibited |
/// | xx1xb |  | digital copy permitted |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Control(u8);

bitflags! {
        impl Control: u8 {
        /// pre-emphasis of 50/15 µs
        const PRE_EMPHASIS = 0b0001;
        /// digital copy-permitted
        const COPY_PERMITTED = 0b0010;
        /// data track
        const DATA_TRACK = 0b0100;
        /// non-stereo audio
        const NON_STEREO_AUDIO = 0b1000;
    }
}

impl Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        bitflags::parser::to_writer_strict(self, f)
    }
}

/// Whether an entry belongs to an audio or a data track, as indicated by the
/// [`Control::DATA_TRACK`] bit of its [`Control`] code.
///
/// Returned by [`Ctrl::kind`]. Prefer `match`ing on this over branching on the
/// [`is_audio`][Ctrl::is_audio] / [`is_data`][Ctrl::is_data] booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum TrackKind {
    /// An audio track: the [`Control::DATA_TRACK`] bit is not set
    Audio,
    /// A data track: the [`Control::DATA_TRACK`] bit is set
    Data,
}

/// Contains a `Control` bitfield which can be queried
pub trait Ctrl {
    /// obtain the relevant Control bitfield
    fn control(&self) -> Control;

    /// whether this is an audio or a data track
    fn kind(&self) -> TrackKind {
        match self.control().contains(Control::DATA_TRACK) {
            true => TrackKind::Data,
            false => TrackKind::Audio,
        }
    }

    /// represents a data track
    fn is_data(&self) -> bool {
        matches!(self.kind(), TrackKind::Data)
    }

    /// represents an audio track
    fn is_audio(&self) -> bool {
        matches!(self.kind(), TrackKind::Audio)
    }
}

impl Ctrl for Control {
    fn control(&self) -> Control {
        *self
    }
}

/// The kind of TocEntry. Returned by [TocEntry::kind] and used in `Result<TocEntries, TocError>::collect()`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum TocEntryKind {
    /// An audio track
    AudioTrack,
    /// A data track
    DataTrack,
    /// The leadout for an audio session
    AudioLeadout,
    /// The leadout for a data session
    DataLeadout,
    /// First track number, encoded as start minute
    FirstAudioTrackInfo,
    /// Last track number, encoded as start minute
    LastAudioTrackInfo,
    /// First track number, encoded as start minute
    FirstDataTrackInfo,
    /// Last track number, encoded as start minute
    LastDataTrackInfo,
    /// An entry type which we do not specifically abstract, not required for audio CD handling
    Unknown,
    /// An entry will all zeros
    Zero,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A collection of [`TocEntry`]s, neatly grouped and sorted.
///
/// Construction is available via `FromIterator<TocEntry> for Result<TocEntries, TocError>`
pub struct TocEntries {
    entries: Vec<TocEntry>,
    leadouts: Leadouts,
}

/// Filtering helpers over iterators of [`TocEntry`] references.
///
/// Automatically implemented for every `Iterator<Item = &TocEntry>`.
pub trait TocEntryFilter<'e>: Iterator<Item = &'e TocEntry> + Sized {
    /// Keep only entries with an audio control code (not a data track), via [`Ctrl::is_audio`].
    fn filter_audio(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| entry.is_audio())
    }

    /// Keep only entries with a data control code, via [`Ctrl::is_data`].
    fn filter_data(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| entry.is_data())
    }

    /// Map consecutive track / lead-out entries to [`Track`]s with audio tracks only, where the
    /// duration of each track is calculated from the start position of the next entry.
    ///
    /// # Notes
    /// - When the next entry is a data track, the fixed offset between audio & data sessions is
    ///   subtracted from the duration.
    fn filter_map_tracks(self) -> impl Iterator<Item = Track<'static>> {
        self.filter(|entry| entry.is_track() || entry.is_leadout())
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then_some(())?;

                let mut duration = next_track.start()?.saturating_sub(this_track.start()?);

                if next_track.is_data_track() {
                    duration = duration.saturating_sub(TocEntries::OFFSET_BEFORE_DATA)
                }

                Some(Track::new(
                    this_track.trackno()?,
                    this_track.start()?,
                    duration,
                ))
            })
            .flatten()
    }
}

impl<'e, I> TocEntryFilter<'e> for I where I: Iterator<Item = &'e TocEntry> + Sized {}

impl TocEntries {
    /// There is a fixed offset between audio & data sessions
    const OFFSET_BEFORE_DATA: Frame = Frame::strict_new(11400);

    /// Provides an iterator over *borrowed* [&TocEntry][TocEntry]s.
    ///
    /// # Note
    ///
    /// [TocEntries] does not directly implement [IntoIterator], but `&TocEntries` does.
    /// Either use `TocEntries.iter()` or `&TocEntries` in a for loop or anywhere else which
    /// expects an `IntoIterator`.
    pub fn iter(&self) -> slice::Iter<'_, TocEntry> {
        self.into_iter()
    }

    /// Provides an iterator over the audio tracks as [`Track`]s with duration calculated based
    /// upon the start position of the next track / leadout.
    ///
    /// # TODO
    /// - TODO: #110 handle discs with data before audio, or multiple audio sessions
    pub fn audio_tracks(&self) -> impl Iterator<Item = Track<'static>> {
        self.iter().filter_map_tracks()
    }

    /// Get a given *audio* track. Returns `None` if the track is not an audio track on this disc.
    pub fn track(&self, track: TrackNo) -> Option<Track<'static>> {
        self.iter()
            .skip_while(|entry| entry.trackno() != Some(track))
            .filter_map_tracks()
            .next()
    }
}

/// Whether only the final leadout is present, or leadouts are available for every session.
///
/// # Interpretation
/// - `Final`: Sessions have not been identified, intra-session leadout positions are unknown
/// - `PerSession(n)`: Sessions are identified, intra-session leadout positions must be available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Leadouts {
    /// Multiple sessions exist but only the final leadout is available
    Final,
    /// Every session has a leadout available, includes the number of sessions found.
    PerSession(NonZeroUsize),
}

impl<'entries> IntoIterator for &'entries TocEntries {
    type Item = &'entries TocEntry;

    type IntoIter = std::slice::Iter<'entries, TocEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<'meta, V> PartialEq<V> for TocEntries
where
    for<'v> &'v V: IntoIterator<Item = &'v Track<'meta>>,
{
    fn eq(&self, tracks: &V) -> bool {
        for pair in self.iter().zip_longest(tracks) {
            match pair {
                EitherOrBoth::Both(TocEntry::Track(entry), track) if entry == &track.toc_entry => {}
                _ => {
                    cold_path();
                    return false;
                }
            }
        }
        true
    }
}

impl TryFrom<TocEntries> for Toc {
    type Error = TocError;

    fn try_from(entries: TocEntries) -> Result<Self, Self::Error> {
        Self::try_from(&entries)
    }
}

impl TryFrom<&TocEntries> for Toc {
    type Error = TocError;

    fn try_from(entries: &TocEntries) -> Result<Self, Self::Error> {
        let audio = entries
            .iter()
            .filter_audio()
            .filter_map(TocEntry::trackentry)
            .map(TrackEntry::start)
            .map(Frame::cast)
            .collect();
        let data = entries
            .iter()
            .rev()
            .filter_data()
            .filter_map(TocEntry::trackentry)
            .map(TrackEntry::start)
            .find_map(Frame::cast);
        let leadout = entries
            .iter()
            .rev()
            .filter_map(TocEntry::leadoutentry)
            .map(LeadoutEntry::start)
            .find_map(Frame::cast)
            .ok_or_else(|| TocError::not_found().context("No leadout found"))?;
        Toc::from_parts(audio, data, leadout).map_err(TocError::from)
    }
}

impl FromIterator<TocEntry> for Result<TocEntries, TocError> {
    fn from_iter<I: IntoIterator<Item = TocEntry>>(iter: I) -> Self {
        let _span = tracing::debug_span!("FromIterator<TocEntry> for Result<TocEntries, TocError>");
        use FirstLast::*;
        use Leadouts::*;
        use TocEntryKind::*;

        let mut entries: Vec<_> = iter.into_iter().collect();

        (!entries.is_empty()).ok_or_else(|| TocError::not_found().context("no TOC entries"))?;

        // INVARIANT: all tracks are expected to be sorted
        // TODO: #190 error constructors should allow closure-free map_err
        entries.try_sort().map_err(|_| {
            TocError::invalid_data().context("Entries appear to be from multiple discs")
        })?;

        if let Some(first_audio_info) = entries
            .iter()
            .filter_audio()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), First))
            .map(InfoEntry::trackno)
        {
            let first_audio_found = entries
                .iter()
                .filter_audio()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(first_audio_info) == first_audio_found).ok_or_else(|| {
                TocError::invalid_data().context(format!(
                    "info entry claims first audio track is number {first_audio_info:?}, but first audio track found is number {first_audio_found:?}"
                ))
            })?;
        }

        if let Some(last_audio_info) = entries
            .iter()
            .rev()
            .filter_audio()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), Last))
            .map(InfoEntry::trackno)
        {
            let last_audio_found = entries
                .iter()
                .rev()
                .filter_audio()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(last_audio_info) == last_audio_found).ok_or_else(|| {
                TocError::invalid_data().context(format!(
                    "info entry claims last audio track is number {last_audio_info:?}, but last audio track found is number {last_audio_found:?}"
                ))
            })?;
        }

        if let Some(first_data_info) = entries
            .iter()
            .filter_data()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), First))
            .map(InfoEntry::trackno)
        {
            let first_data_found = entries
                .iter()
                .filter_data()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(first_data_info) == first_data_found).ok_or_else(|| {
                TocError::invalid_data().context(format!(
                    "info entry claims first data track is number {first_data_info:?}, but first data track found is number {first_data_found:?}"
                ))
            })?;
        }

        if let Some(last_data_info) = entries
            .iter()
            .rev()
            .filter_data()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), Last))
            .map(InfoEntry::trackno)
        {
            let last_data_found = entries
                .iter()
                .rev()
                .filter_data()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(last_data_info) == last_data_found).ok_or_else(|| {
                TocError::invalid_data().context(format!(
                    "info entry claims last data track is number {last_data_info:?}, but last data track found is number {last_data_found:?}"
                ))
            })?;
        }

        let has_entries_without_session = entries
            .iter()
            .map(|entry| entry.session())
            .find(Option::is_none)
            .is_some();
        let has_session_info = entries
            .iter()
            .map(|entry| entry.session())
            .find(Option::is_some)
            .is_some();

        (has_session_info != has_entries_without_session).ok_or_else(|| {
            TocError::invalid_data().context("Some entries have session info, others do not")
        })?;

        let sessions = has_session_info
            .then(|| entries.iter().map(|entry| entry.session()).dedup().count())
            .and_then(NonZeroUsize::new);

        let leadout_count: NonZeroUsize = entries
            .iter()
            .filter_map(TocEntry::leadoutentry)
            .count()
            .try_into()
            .map_err(|_| TocError::invalid_data().context("No leadout entries found"))?;

        let leadouts = match (sessions, leadout_count) {
            (None, NonZeroUsize::MIN) => Leadouts::Final,
            (None, l) => {
                return Err(TocError::invalid_data()
                    .context(format!("found {l} leadouts but no session information")));
            }
            (Some(s), l) if s == l => Leadouts::PerSession(s),
            (Some(s), l) => {
                // s != l
                return Err(TocError::invalid_data()
                    .context(format!("found {s} sessions but {l} leadout entries")));
            }
        };

        entries
            .iter()
            .map_windows(|[this_entry, next_entry]| (*this_entry, *next_entry))
            .try_for_each(|(this_entry, next_entry)| {
                match (this_entry.kind(), next_entry.kind(), leadouts) {
                    (FirstAudioTrackInfo, LastAudioTrackInfo, PerSession(_)) |
                    (LastAudioTrackInfo, AudioTrack, PerSession(_)) |
                    (AudioTrack, AudioTrack, _) |
                    (AudioTrack, DataTrack, Final) |
                    (AudioTrack, AudioLeadout, _) |
                    (AudioLeadout, Unknown, PerSession(_)) |
                    (AudioLeadout, FirstAudioTrackInfo, PerSession(_)) |
                    (AudioLeadout, FirstDataTrackInfo, PerSession(_)) |
                    (FirstDataTrackInfo, LastDataTrackInfo, PerSession(_)) |
                    (LastDataTrackInfo, DataTrack, PerSession(_)) |
                    (DataTrack, DataTrack, Final) |
                    (DataTrack, AudioTrack, Final) |
                    (DataTrack, DataLeadout, _) |
                    (DataLeadout, Unknown, PerSession(_)) |
                    (DataLeadout, FirstAudioTrackInfo, PerSession(_)) |
                    (DataLeadout, FirstDataTrackInfo, PerSession(_)) |
                    (Unknown, Unknown, PerSession(_)) |
                    (Unknown, FirstAudioTrackInfo, PerSession(_)) |
                    (Unknown, FirstDataTrackInfo, PerSession(_))
                    => Ok(()),
                    (this, next, _) => {dbg!(this_entry); dbg!(next_entry);Err(TocError::invalid_data().context(
                        format!("invalid toc entry order. {this} cannot be followed by {next} with leadouts {leadouts:?}"),
                    ))},
                }
            })?;

        let tocentries = TocEntries { entries, leadouts };

        Ok(tocentries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    use crate::LEADIN;

    #[test]
    fn usable_in_const_context() {
        // compile-time proof: these only compile if TrackNo & Point are usable in const contexts
        const ONE: TrackNo = TrackNo::strict_new(1);
        const MAX: TrackNo = TrackNo::MAX;
        const NEW_OK: Option<TrackNo> = TrackNo::new(42_u8);
        const NEW_ZERO: Option<TrackNo> = TrackNo::new(0_u8);
        const NEW_OVER: Option<TrackNo> = TrackNo::new(100_u8);
        const CAST: usize = ONE.cast();
        const AS_U8: u8 = ONE.cast();
        const EQ: bool = ONE == TrackNo::strict_new(1);
        const NE: bool = ONE != TrackNo::strict_new(2);
        const LT: bool = ONE < MAX;
        const LEADIN_KIND: PointKind = Point(0xAA).kind();
        const LEADIN_IS_LEADIN: bool = b0000::LEADOUT_TRACK_ID.is_leadin();
        const A0_IS_INFO: bool = b0010::FIRST_TRACK_ID.is_info();
        const A0_INFO_KIND: Option<FirstLast> = b0010::FIRST_TRACK_ID.info_kind();

        // cross-check the const-evaluated values against runtime evaluation
        let one = TrackNo::strict_new(1);
        assert_eq!(NEW_OK, TrackNo::new(42_u8));
        assert_eq!(NEW_ZERO, None);
        assert_eq!(NEW_OVER, None);
        assert_eq!(CAST, one.cast::<usize>());
        assert_eq!(AS_U8, one.cast::<u8>());
        assert_eq!(EQ, one == TrackNo::strict_new(1));
        assert_eq!(NE, one != TrackNo::strict_new(2));
        assert_eq!(LT, one < TrackNo::MAX);
        assert_eq!(LEADIN_KIND, Point(0xAA).kind());
        assert_eq!(LEADIN_IS_LEADIN, b0000::LEADOUT_TRACK_ID.is_leadin());
        assert_eq!(A0_IS_INFO, b0010::FIRST_TRACK_ID.is_info());
        assert_eq!(A0_INFO_KIND, Some(FirstLast::First));
    }

    #[rstest]
    #[case(Point(0x00), false, false, false, None, PointKind::Unknown)]
    #[case(Point(0x01), true, false, false, None, PointKind::Normal)]
    #[case(Point(42), true, false, false, None, PointKind::Normal)]
    #[case(Point(99), true, false, false, None, PointKind::Normal)]
    #[case(Point(100), false, false, false, None, PointKind::Unknown)]
    #[case(b0000::LEADOUT_TRACK_ID, false, true, false, None, PointKind::LeadIn)]
    #[case(b0010::LEADOUT_TRACK_ID, false, true, false, None, PointKind::LeadIn)]
    #[case(
        b0010::FIRST_TRACK_ID,
        false,
        false,
        true,
        Some(FirstLast::First),
        PointKind::FirstTrackInfo
    )]
    #[case(
        b0010::LAST_TRACK_ID,
        false,
        false,
        true,
        Some(FirstLast::Last),
        PointKind::LastTrackInfo
    )]
    #[case(Point(0xB0), false, false, false, None, PointKind::Unknown)]
    fn point_classification(
        #[case] point: Point,
        #[case] is_normal: bool,
        #[case] is_leadin: bool,
        #[case] is_info: bool,
        #[case] info_kind: Option<FirstLast>,
        #[case] kind: PointKind,
    ) {
        assert_eq!(point.is_normal(), is_normal);
        assert_eq!(point.is_leadin(), is_leadin);
        assert_eq!(point.is_info(), is_info);
        assert_eq!(point.info_kind(), info_kind);
        assert_eq!(point.kind(), kind);
    }

    #[test]
    fn track_entry_new_stores_all_fields() {
        let entry = TrackEntry::new(
            TrackNo::strict_new(1),
            LEADIN,
            Control::default(),
            Some(NonZeroU8::MIN),
        );
        assert_eq!(
            (entry.trackno(), entry.start(), entry.session()),
            (TrackNo::strict_new(1), LEADIN, Some(NonZeroU8::MIN))
        );

        // special point values (lead-out / track-info codes) are not track numbers
        assert!(!b0000::LEADOUT_TRACK_ID.is_normal());
        assert!(!b0010::FIRST_TRACK_ID.is_normal());

        // start is deliberately not validated against the lead-in: copy-protected discs
        // (e.g. Cactus Data Shield, Key2Audio) store track starts within the lead-in
        let entry = TrackEntry::new(
            TrackNo::strict_new(1),
            Frame::strict_new(149_u32),
            Control::default(),
            None,
        );
        assert_eq!(entry.start(), Frame::strict_new(149_u32));
    }

    #[test]
    fn leadout_entry_new_stores_all_fields() {
        // start is deliberately not validated against the lead-in (copy-protected discs)
        let entry = LeadoutEntry::new(LEADIN, Control::default(), None);
        assert_eq!(
            (entry.start(), entry.control(), entry.session()),
            (LEADIN, Control::default(), None)
        );
    }

    #[test]
    fn info_entry_new_stores_all_fields() {
        let entry = InfoEntry::new(
            NonZeroU8::MIN,
            TrackNo::strict_new(1),
            Control::default(),
            FirstLast::First,
        );
        assert_eq!(
            (entry.session(), entry.trackno(), entry.info_type()),
            (NonZeroU8::MIN, TrackNo::strict_new(1), FirstLast::First)
        );
    }

    #[test]
    fn raw_toc_entry_new_stores_all_fields() {
        let msf = Msf::new(0, 2, 0);
        let pmsf = Msf::new(1, 2, 3);
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            2,
            Point(3),
            msf,
            0,
            pmsf,
        );
        assert_eq!(
            (
                raw.session(),
                raw.adr(),
                raw.control(),
                raw.tno(),
                raw.point(),
                raw.msf(),
                raw.zero(),
                raw.pmsf()
            ),
            (
                Some(NonZeroU8::MIN),
                1,
                Control::default(),
                2,
                Point(3),
                msf,
                0,
                pmsf
            )
        );
    }

    #[test]
    fn raw_entries_classify_via_validating_constructors() {
        let msf = Msf::new(0, 2, 0x21);
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        let entry = Option::<TocEntry>::from(raw).unwrap();
        assert!(matches!(entry, TocEntry::Track(_)));
        assert_eq!(entry.trackno(), Some(TrackNo::strict_new(1)));
        assert_eq!(entry.start(), Some(Frame::from(msf)));

        // a track start within the lead-in (as stored by copy-protected discs) must still
        // classify as a track rather than failing
        let msf = Msf::new(0, 0, 5);
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Track(_))
        ));
    }

    #[test]
    fn unmodelled_raw_entry_points_become_unknown() {
        let msf = Msf::new(0, 2, 0);
        // a point which is not a normal track number / special code is valid but unmodelled
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(0xB0), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Unknown(_))
        ));
    }

    #[test]
    fn invalid_track_info_entries_are_rejected_not_unknown() {
        let msf = Msf::new(0, 2, 0);
        // A0 point without session info is recognizably a track-info entry, but invalid
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert_eq!(Option::<TocEntry>::from(raw), None);
        // A0 point with a non-normal track number in PMIN is likewise invalid
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(0xAA, 0, 0),
        );
        assert_eq!(Option::<TocEntry>::from(raw), None);
    }

    #[rstest]
    #[case(Point(1), Control::default(), TocEntryKind::AudioTrack)]
    #[case(Point(99), Control::default(), TocEntryKind::AudioTrack)]
    #[case(
        Point(1),
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::DataTrack
    )]
    #[case(
        b0000::LEADOUT_TRACK_ID,
        Control::default(),
        TocEntryKind::AudioLeadout
    )]
    #[case(
        b0010::LEADOUT_TRACK_ID,
        Control::default(),
        TocEntryKind::AudioLeadout
    )]
    #[case(
        b0000::LEADOUT_TRACK_ID,
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::DataLeadout
    )]
    #[case(
        b0010::FIRST_TRACK_ID,
        Control::default(),
        TocEntryKind::FirstAudioTrackInfo
    )]
    #[case(
        b0010::LAST_TRACK_ID,
        Control::default(),
        TocEntryKind::LastAudioTrackInfo
    )]
    #[case(
        b0010::FIRST_TRACK_ID,
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::FirstDataTrackInfo
    )]
    #[case(
        b0010::LAST_TRACK_ID,
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::LastDataTrackInfo
    )]
    #[case(Point(0xB0), Control::default(), TocEntryKind::Unknown)]
    // ADR != 1 is always Unknown
    #[case(Point(1), Control::default(), TocEntryKind::Unknown)]
    fn raw_toc_entry_kind_classifies_by_point_and_control(
        #[case] point: Point,
        #[case] control: Control,
        #[case] expected: TocEntryKind,
    ) {
        let msf = Msf::new(0, 2, 0);
        let adr = if expected == TocEntryKind::Unknown && point.is_normal() && point != Point(0xB0)
        {
            0 // force ADR != 1 for the "ADR != 1" case
        } else {
            1
        };
        let raw = RawTocEntry::new(None, adr, control, 0, point, msf, 0, msf);
        assert_eq!(raw.kind(), expected);
    }

    #[test]
    fn try_from_raw_for_track_entry_succeeds_for_normal_point() {
        let msf = Msf::new(0, 2, 0x21);
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(3), msf, 0, msf);
        let entry = TrackEntry::try_from(raw).unwrap();
        assert_eq!(entry.trackno(), TrackNo::strict_new(3));
        assert_eq!(entry.start(), Frame::from(msf));
    }

    #[test]
    fn try_from_raw_for_track_entry_fails_for_non_track_points() {
        let msf = Msf::new(0, 2, 0);
        // leadout point
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0000::LEADOUT_TRACK_ID,
            msf,
            0,
            msf,
        );
        assert!(TrackEntry::try_from(raw).is_err());
        // info point
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert!(TrackEntry::try_from(raw).is_err());
        // ADR != 1
        let raw = RawTocEntry::new(None, 0, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(TrackEntry::try_from(raw).is_err());
    }

    #[test]
    fn try_from_raw_for_leadout_entry_succeeds_for_leadout_points() {
        let msf = Msf::new(0, 2, 0);
        for point in [b0000::LEADOUT_TRACK_ID, b0010::LEADOUT_TRACK_ID] {
            let raw = RawTocEntry::new(None, 1, Control::default(), 0, point, msf, 0, msf);
            let entry = LeadoutEntry::try_from(raw).unwrap();
            assert_eq!(entry.start(), Frame::from(msf));
        }
    }

    #[test]
    fn try_from_raw_for_leadout_entry_fails_for_non_leadout_points() {
        let msf = Msf::new(0, 2, 0);
        // normal track point
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(LeadoutEntry::try_from(raw).is_err());
        // ADR != 1
        let raw = RawTocEntry::new(
            None,
            0,
            Control::default(),
            0,
            b0000::LEADOUT_TRACK_ID,
            msf,
            0,
            msf,
        );
        assert!(LeadoutEntry::try_from(raw).is_err());
    }

    #[test]
    fn try_from_raw_for_info_entry_succeeds_for_valid_a0_a1() {
        let msf = Msf::new(0, 2, 0);
        // FIRST_TRACK_ID with valid session and normal track in PMIN
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        let entry = InfoEntry::try_from(raw).unwrap();
        assert_eq!(entry.info_type(), FirstLast::First);
        assert_eq!(entry.trackno(), TrackNo::strict_new(1));
        assert_eq!(entry.session(), NonZeroU8::MIN);

        // LAST_TRACK_ID
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::LAST_TRACK_ID,
            msf,
            0,
            Msf::new(5, 0, 0),
        );
        let entry = InfoEntry::try_from(raw).unwrap();
        assert_eq!(entry.info_type(), FirstLast::Last);
        assert_eq!(entry.trackno(), TrackNo::strict_new(5));
    }

    #[test]
    fn try_from_raw_for_info_entry_fails_for_invalid() {
        let msf = Msf::new(0, 2, 0);
        // no session
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert!(InfoEntry::try_from(raw).is_err());
        // non-normal track in PMIN
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(0xAA, 0, 0),
        );
        assert!(InfoEntry::try_from(raw).is_err());
        // not an info point
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(InfoEntry::try_from(raw).is_err());
    }

    #[test]
    fn from_raw_glues_kind_and_tryfrom_back_together() {
        let msf = Msf::new(0, 2, 0x21);

        // track
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Track(_))
        ));

        // leadout (b0000)
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0000::LEADOUT_TRACK_ID,
            msf,
            0,
            msf,
        );
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Leadout(_))
        ));

        // info
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::TrackInfo(_))
        ));

        // unknown
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(0xB0), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Unknown(_))
        ));

        // invalid info (no session) → None
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert_eq!(Option::<TocEntry>::from(raw), None);
    }

    #[rstest]
    #[case(0_u8, None)]
    #[case(1_u8, Some(TrackNo::strict_new(1)))]
    #[case(42_u8, Some(TrackNo::strict_new(42)))]
    #[case(99_u8, Some(TrackNo::MAX))]
    #[case(100_u8, None)]
    #[case(0xAA_u8, None)]
    // wider integers are accepted via checked cast, rejected if they do not fit in a u8
    #[case(1_u16, Some(TrackNo::strict_new(1)))]
    #[case(99_u32, Some(TrackNo::strict_new(99)))]
    #[case(100_u16, None)]
    #[case(256_u16, None)]
    #[case(u16::MAX, None)]
    #[case(-1_i32, None)]
    fn new_only_accepts_valid_track_numbers<T>(#[case] value: T, #[case] expected: Option<TrackNo>)
    where
        u8: CheckedCastFromInt<T>,
    {
        assert_eq!(TrackNo::new(value), expected);
    }

    #[test]
    fn strict_and_saturating_new_behave() {
        assert_eq!(TrackNo::strict_new(99_u8), TrackNo::MAX);
        assert_eq!(TrackNo::saturating_new(0_u8), TrackNo::strict_new(1));
        assert_eq!(TrackNo::saturating_new(100_u8), TrackNo::MAX);
        assert_eq!(TrackNo::saturating_new(256_u16), TrackNo::MAX);
        assert_eq!(TrackNo::saturating_new(-1_i32), TrackNo::strict_new(1));
    }
}
