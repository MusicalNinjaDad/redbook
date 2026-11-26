//! Provides [`TrackNo`], a semantically validated track number, and the supporting [`Point`] for
//! the related multi-use field in TOC entries.

use std::{
    convert::{BoundedCastFromInt, CheckedCastFromInt},
    fmt::Display,
    marker::StructuralPartialEq,
    num::NonZeroU8,
    ops::{Deref, DerefMut},
};

use derive_more::Display;

use crate::toc::{FirstLast, InvalidTrackNo};

/// POINT field identifiers used by READ TOC/PMA format 0b0000 (MMC-3, section 5.23.2)
mod b0000 {
    use super::Point;
    /// 0b0000 format POINT identifier for Leadout
    pub(super) const LEADOUT_TRACK_ID: Point = Point(0xAA);
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
mod b0010 {
    use super::Point;
    /// 0b0010 format POINT identifier for the first track number of a session
    pub(super) const FIRST_TRACK_ID: Point = Point(0xA0);
    /// 0b0010 format POINT identifier for the last track number of a session
    pub(super) const LAST_TRACK_ID: Point = Point(0xA1);
    /// 0b0010 format POINT identifier for Leadout
    pub(super) const LEADOUT_TRACK_ID: Point = Point(0xA2);
}

impl super::Format {
    /// The [`Point`] value representing "lead-out" for this format
    pub fn leadout(&self) -> Point {
        match self {
            Self::B0000 => b0000::LEADOUT_TRACK_ID,
            Self::B0010(_) => b0010::LEADOUT_TRACK_ID,
        }
    }

    /// The [`Point`] value representing "first track number" for this format
    pub fn first_track_info(&self) -> Option<Point> {
        match self {
            Self::B0000 => None,
            Self::B0010(_) => Some(b0010::FIRST_TRACK_ID),
        }
    }

    /// The [`Point`] value representing "last track number" for this format
    pub fn last_track_info(&self) -> Option<Point> {
        match self {
            Self::B0000 => None,
            Self::B0010(_) => Some(b0010::LAST_TRACK_ID),
        }
    }
}

/// A raw POINT field value from a TOC track descriptor (Q sub-channel / READ TOC), exactly as read
/// from the disc.
///
/// Any `u8` is a valid `Point`: POINT values include "normal" track numbers (1..=99, see
/// [`TrackNo`]) as well as "special" codes such as the lead-out ([`b0000::LEADOUT_TRACK_ID`],
/// [`b0010::LEADOUT_TRACK_ID`]) or the first/last track info of a session
/// ([`b0010::FIRST_TRACK_ID`], [`b0010::LAST_TRACK_ID`]). Use [`is_normal()`][Self::is_normal],
/// [`is_leadin()`][Self::is_leadin], [`is_info()`][Self::is_info], [`info_kind()`][Self::info_kind]
/// or [`kind()`][Self::kind] for classification.
///
/// # Notes
/// - This is the "lazy" counterpart to the fully-encapsulated [`TrackNo`]: construction is via
///   `Point(n)` / `From<u8>` for simplicity, `Deref` & `DerefMut` are implemented with `Target =
///   u8`, and `repr(transparent)` so a `Point` can be used interchangeably with a `u8` in both rust
///   and ffi code.
/// - Use [`cast()`][Self::cast] when converting to other integers
/// - `PartialEq` & `PartialOrd` are deliberately not implemented between `Point` & `u8` to ensure
///   comparisons don't inadvertently compare a Point with something semantically different
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
#[repr(transparent)]
pub struct Point(pub(crate) u8);

impl Point {
    /// Whether this is a "normal" track number, i.e. convertible to a [`TrackNo`] (1..=99)
    pub const fn is_normal(&self) -> bool {
        TrackNo::checked_new(self.0).is_some()
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
/// assert_eq!(TrackNo::new(7_u16), Ok(TrackNo::strict_new(7)));
/// assert!(TrackNo::new(0_u8).is_err());
/// assert!(TrackNo::new(100_u8).is_err());
/// assert!(TrackNo::new(256_u16).is_err());
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

    /// Creates a new `TrackNo` from any integer type, returning [`InvalidTrackNo`] (carrying the
    /// offending value) if the value does not fit in a `u8` (track numbers are stored on disc in
    /// a single byte) or is outside 1..=[`Self::MAX`].
    ///
    /// # Note
    /// - For const contexts use [`Self::checked_new`]
    /// - For a panicking constructor use [`Self::strict_new`]
    /// - For a saturating constructor use [`Self::saturating_new`]
    pub fn new<N>(value: N) -> Result<Self, InvalidTrackNo>
    where
        u8: TryFrom<N>,
        i128: From<N>,
        N: Copy,
    {
        u8::try_from(value)
            .ok()
            .and_then(Self::from_u8)
            .and_then(Self::validate)
            .ok_or_else(|| InvalidTrackNo::new(value))
    }

    /// Creates a new `TrackNo` from any integer type, returning `None` if the value does not
    /// fit in a `u8` (track numbers are stored on disc in a single byte) or is outside
    /// 1..=[`Self::MAX`].
    ///
    /// # Note
    /// - For a `Result`-returning constructor use [`Self::new`]
    /// - For a panicking constructor use [`Self::strict_new`] directly, not `Self::checked_new(...).unwrap()`
    /// - For a saturating constructor use [`Self::saturating_new`]
    pub const fn checked_new<N>(value: N) -> Option<Self>
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
        match Self::from_u8(u8::strict_cast_from(value)) {
            Some(track_no) => track_no.assert_valid(),
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
        // Cannot panic: saturating_cast_from(value).max(1) is always >= 1
        Self::from_u8(u8::saturating_cast_from(value).max(1))
            .unwrap()
            .saturate()
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
    const fn is_valid(&self) -> bool {
        self.0.get() <= Self::MAX.0.get()
    }

    /// Validates `self` and returns `None` if invalid.
    ///
    /// Private as no-one else should be able to construct an invalid TrackNo
    const fn validate(self) -> Option<Self> {
        self.is_valid().then_some(self)
    }

    /// Validates `self`, panicking if invalid
    ///
    /// # Panics
    /// Panics if `self` is above [`Self::MAX`].
    const fn assert_valid(self) -> Self {
        assert!(self.is_valid(), "semantic overflow");
        self
    }

    /// Replaces invalid values with [`Self::MAX`]. No need to saturate at 0 as this is covered
    /// by the internal representation.
    const fn saturate(self) -> Self {
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

impl TryFrom<Point> for TrackNo {
    type Error = InvalidTrackNo;

    fn try_from(point: Point) -> Result<Self, Self::Error> {
        Self::new(point.0)
    }
}

impl Display for TrackNo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "track: {n}", n = self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use rstest::rstest;

    #[test]
    fn usable_in_const_context() {
        // compile-time proof: these only compile if TrackNo & Point are usable in const contexts
        const ONE: TrackNo = TrackNo::strict_new(1);
        const MAX: TrackNo = TrackNo::MAX;
        const NEW_OK: Option<TrackNo> = TrackNo::checked_new(42_u8);
        const NEW_ZERO: Option<TrackNo> = TrackNo::checked_new(0_u8);
        const NEW_OVER: Option<TrackNo> = TrackNo::checked_new(100_u8);
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
        assert_eq!(NEW_OK, TrackNo::checked_new(42_u8));
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
}
