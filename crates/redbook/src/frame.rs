//! Time units: Audio CDs measure time in 1/75 second "frames". Provides `Frame` & `Msf`
//! (min, sec, frame).

use std::{
    convert::{BoundedCastFromInt, CheckedCastFromInt},
    fmt::{Debug, Display},
    marker::Destruct,
    num::TryFromIntError,
    ops::{Div, Rem},
};

use crate::LEADIN;

/// CD audio frame (1/75 sec). Basic unit of time for CD audio.
///
/// A frame represents a single unit of CD audio data, which is 1/75th of a second.
/// This is the fundamental unit of time measurement for CD audio.
///
/// # Note
///
/// - Internally frames are stored as u32. The maximum frame size in a real-world situation
///   is limited by the (u8, u8, u8) representation of MSF when stored on a disc. Given that
///   (0xFF * 60 * 75) = 0x11826C, 3 bytes are needed to store 0xFF minutes, so a u16 is too small and
///   anything over u32 is oversized.
///
/// # Invariant
///
/// A `Frame` can always be converted into an [`Msf`] representation: the stored value is always
/// `<=` [`Frame::MAX`]. This is upheld by validating construction ([`Frame::new`] &
/// [`Frame::strict_new`]) and by every arithmetic method below, so that [`From<Frame> for Msf`]
/// is always well-defined.
///
/// # Arithmetic
///
/// `+`, `-`, `*`, `/`, `%`, `+=` etc. are deliberately not implemented for `Frame` (or [`Msf`]).
/// Instead use the `saturating_*`, `wrapping_*`, `checked_` & `strict_*` methods, which accept
/// only `rhs: Frame` or `rhs: Msf` for add/sub/rem, and only a scalar `N` (where `u32: From<N>`)
/// for mul/div. If an integer result is required, use [`Frame::cast`] first and then the
/// corresponding `u32` method.
///
/// # TODOs
/// - TODO: #106 Repr transparent, store u-whatever, as_blah casts, etc for Frame, TrackNo, etc...
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct Frame(u32);

/// TODO: #124 currently outputs "Frame n" should probably just be "n", or 2 types: position & duration
impl Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame {}", self.0)
    }
}

const impl UpperBounded for Frame {
    type Inner = u32;

    /// Given [constant improvements in manufacturing][wiki], this represent the maximum Frame value
    /// which can be represented in a TOC or other sub-channel data which stores times as
    /// MSF(u8,u8,u8). It is certainly slightly oversized, but some crazy person may choose to ignore
    /// that there are only 60 seconds in a minute and store an MSF of (FF, FF, FF) for some reason.
    ///
    /// [wiki]: https://en.wikipedia.org/wiki/Compact_Disc_Digital_Audio#Storage_capacity_and_playing_time
    const MAX: Self = Frame((0xFF * 60 * 75) + (0xFF * 75) + 0xFF);

    const ZERO: Self = Frame(0);

    fn into_inner(self) -> Self::Inner {
        self.0
    }

    #[expect(unsafe_code, reason = "default impls")]
    unsafe fn new_unchecked(inner: Self::Inner) -> Self {
        Self(inner)
    }
}

impl Frame {
    /// Returns a frame relative to the lead-in position.
    ///
    /// This subtracts the standard 150-frame lead-in from the frame position,
    /// giving the position relative to the start of the actual audio data.
    ///
    /// # Note
    ///
    /// - To guard against miguided attempts to "copy protect" CDs by breaking them, this will
    ///   never return negative. So a badly constructed TOC with track 1 starting at frame 147
    ///   (0m:1s:72f) will return frame 0 relative to leadin.
    ///
    /// # TODO
    /// - TODO #154 what is the right duration for a 1st track with start frame 147?
    pub const fn relative_to_leadin(self) -> Self {
        self.saturating_sub(LEADIN)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// CD audio time in min:sec:frame format (75 frames/sec).
///
/// MSF (Minute-Second-Frame) is a common format for representing CD audio positions and is used
/// in the table of contents stored in the lead-in of every disc.
///
/// # Notes
///
/// - `min` is `u8`, and therefore restricted to 256, as this is designed to only store values
///   which can be meaningfully represented on a CD. The technical format used on discs only
///   provides 1 byte for storing mins.
/// - `Msf` does not perform any validation that sec < 60 or frames < 75. Converting a
///   value such as `Msf(1, 60, 75)` to [`Frame`]s and back will result in the more meaningful
///   and correct `Msf(2, 1, 0)`.
/// - Downstream implementors of conversions and users of Msf should pay attention to handling
///   technically correct but semantically invalid cases, such as the example above.
/// - It is recommended to use [`Frame`] as the primary timescale to avoid such semantic
///   concerns leading to hidden logic errors.
///
/// # Arithmetic
///
/// `+`, `-`, `*`, `/`, `%` etc. are deliberately not implemented for `Msf`. Use the
/// `saturating_*`, `wrapping_*`, `checked_` & `strict_*` methods instead. These accept only
/// `rhs: Frame` or `rhs: Msf` for add/sub/rem, and only a scalar `N` (where `u8: From<N>`) for
/// mul/div. Arithmetic is computed via [`Frame`], so results are always normalised
/// (e.g. `Msf::new(1, 60, 75).strict_add(...)`) and any [`Frame::MAX`] bound is respected.
///
///  # TODO
/// - impl Display
pub struct Msf {
    /// Minutes component. This is stored as a u8 in the physical disc TOC.
    pub(crate) min: u8,
    /// Seconds component. (Not validated to be ..=59)
    sec: u8,
    /// Frames component. (Not validated to be ..=74)
    frame: u8,
}

impl Msf {
    /// Creates a new `Msf` from minute, second, and frame components.
    ///
    /// # Note
    ///
    /// - This does not perform any validation that sec < 60 or frames < 75. Converting a
    ///   value such as `Msf(1, 60, 75)` to [`Frame`]s and back will result in the more meaningful
    ///   and correct `Msf(2, 1, 0)`.
    /// - Downstream implementors of conversions and users of Msf should pay attention to handling
    ///   technically correct but semantically invalid cases, such as the example above.
    /// - It is recommended to use [`Frame`] as the primary timescale to avoid such semantic
    ///   concerns leading to hidden logic errors.
    pub const fn new(min: u8, sec: u8, frame: u8) -> Self {
        Self { min, sec, frame }
    }

    /// Returns an MSF value relative to the lead-in position.
    ///
    /// This subtracts the standard 150-frame (2-second) lead-in from the MSF value,
    /// giving the position relative to the start of the actual audio data.
    ///
    /// # Note
    ///
    /// - this leverages [Frame::relative_to_leadin], and will also never return negative. See
    ///   the comment there for more details.
    pub const fn relative_to_leadin(self) -> Self {
        Self::from(<Frame as From<Msf>>::from(self).relative_to_leadin())
    }

    // ============================
    // Arithmetic: add / sub / rem
    //
    // rhs is restricted by From
    // which only allows Frame & Msf
    //
    // Subordinates to Frame to handle
    // degenerate cases where secs / frames
    // need to saturate above 60 / 75
    // ============================

    /// Strict addition. Computes `self + rhs`, panicking on overflow.
    ///
    /// # Note
    /// - `rhs` may be a [`Frame`] or an [`Msf`].
    /// -  "degenerate" cases will saturate as described TODO #156 denegerate docs
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    pub const fn strict_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let lhs = <Frame as From<Msf>>::from(self);
        let frames = lhs.strict_add(rhs);
        Self::from(frames)
    }

    /// Checked addition. Computes `self + rhs`, returning `None` if the result is above
    /// `Msf(FF,FF,FF)`
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_add<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        let lhs = <Frame as From<Msf>>::from(self);
        let frames = lhs.checked_add(rhs);
        frames.map(Into::into)
    }

    /// Saturating addition. Computes `self + rhs`, saturating at `Msf(FF,FF,FF)`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let lhs = <Frame as From<Msf>>::from(self);
        let frames = lhs.saturating_add(rhs);
        Self::from(frames)
    }

    /// Strict subtraction. Computes `self - rhs`, panicking if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::strict_sub`].
    pub const fn strict_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let lhs = <Frame as From<Msf>>::from(self);
        let frames = lhs.strict_sub(rhs);
        Self::from(frames)
    }

    /// Checked subtraction. Computes `self - rhs`, returning `None` if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_sub<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        let lhs = <Frame as From<Msf>>::from(self);
        let frames = lhs.checked_sub(rhs);
        frames.map(Into::into)
    }

    /// Saturating subtraction. Computes `self - rhs`, saturating at zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let lhs = <Frame as From<Msf>>::from(self);
        let frames = lhs.saturating_sub(rhs);
        Self::from(frames)
    }

    // ============================
    // Arithmetic: mul / div
    //
    // rhs may only be a scalar N
    // deliberately not Frame/Msf as
    // we don't expect acceleration
    // calulations
    // ============================

    /// Strict multiplication. Computes `self * rhs`, panicking if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::strict_mul`].
    pub const fn strict_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        let lhs = Frame::from(self);
        let frames = lhs.strict_mul(rhs);
        Self::from(frames)
    }

    /// Checked multiplication. Computes `self * rhs`, returning `None` if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn checked_mul<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        let lhs = Frame::from(self);
        let frames = lhs.checked_mul(rhs);
        frames.map(Into::into)
    }

    /// Saturating multiplication. Computes `self * rhs`, saturating at [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn saturating_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
        N: [const] Destruct,
    {
        let lhs = Frame::from(self);
        let frames = lhs.saturating_mul(rhs);
        Self::from(frames)
    }

    /// Basic division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_div & strict_div, as self is valid and the result of
    ///   divison must always be 0..=self
    pub const fn div<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.div(rhs);
        Self::from(frames)
    }

    /// Checked division. Computes `self / rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.checked_div(rhs);
        frames.map(Into::into)
    }

    /// Basic remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_rem & strict_rem, as self is valid and the remainder from
    ///   divison must always be 0..=self
    pub const fn rem<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.rem(rhs);
        Self::from(frames)
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be any scalar integer.
    ///
    /// # Note
    /// - This is the only specialised remainder. Use `rem` if you can accept a panic on div by zero.
    pub const fn checked_rem<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.checked_rem(rhs);
        frames.map(Into::into)
    }
}

/// Msf is stored as 1-byte binary-encoded-decimal.
#[expect(non_camel_case_types, reason = "like i8, u8")]
#[expect(unused, reason = "WIP")]
pub struct bcd8(u8);

impl bcd8 {
    /// BCD encodes 0..=9 in 4-bits. Therefore a 1-byte Bcd can represent 0..=99.
    pub const MAX: Self = Self(99);
}

/// Integer types that implement checked_, saturating_, etc such that they are suitable for
/// wrapping in an [`UpperBounded`]
///
/// # TODO
/// - TODO #160 use `num_traits` when it adds `const` to the traits
#[expect(missing_docs, reason = "shadowing inherent methods")]
pub const trait CheckedMath
where
    Self: Sized
        + [const] CheckedCastFromInt<Self>
        + [const] BoundedCastFromInt<Self>
        + [const] Div<Self, Output = Self>
        + [const] Rem<Self, Output = Self>,
{
    fn checked_add(self, rhs: Self) -> Option<Self>;
    fn saturating_add(self, rhs: Self) -> Self;
    fn strict_add(self, rhs: Self) -> Self;
    fn checked_sub(self, rhs: Self) -> Option<Self>;
    fn saturating_sub(self, rhs: Self) -> Self;
    fn strict_sub(self, rhs: Self) -> Self;
    fn checked_mul(self, rhs: Self) -> Option<Self>;
    fn saturating_mul(self, rhs: Self) -> Self;
    fn strict_mul(self, rhs: Self) -> Self;
    fn checked_div(self, rhs: Self) -> Option<Self>;
    fn checked_rem(self, rhs: Self) -> Option<Self>;
}

const impl CheckedMath for u32 {
    fn checked_add(self, rhs: Self) -> Option<Self> {
        self.checked_add(rhs)
    }

    fn saturating_add(self, rhs: Self) -> Self {
        self.saturating_add(rhs)
    }

    fn strict_add(self, rhs: Self) -> Self {
        self.strict_add(rhs)
    }

    fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.checked_sub(rhs)
    }

    fn saturating_sub(self, rhs: Self) -> Self {
        self.saturating_sub(rhs)
    }

    fn strict_sub(self, rhs: Self) -> Self {
        self.strict_sub(rhs)
    }

    fn checked_mul(self, rhs: Self) -> Option<Self> {
        self.checked_mul(rhs)
    }

    fn saturating_mul(self, rhs: Self) -> Self {
        self.saturating_mul(rhs)
    }

    fn strict_mul(self, rhs: Self) -> Self {
        self.strict_mul(rhs)
    }

    fn checked_div(self, rhs: Self) -> Option<Self> {
        self.checked_div(rhs)
    }

    fn checked_rem(self, rhs: Self) -> Option<Self> {
        self.checked_rem(rhs)
    }
}

/// A semantic wrapper around a primitive int type, which has a specific upper bound, lower than
/// the wrapped type.
pub const trait UpperBounded
where
    Self: Sized + [const] PartialOrd + [const] Destruct,
{
    /// The underlying int primitive
    type Inner: [const] std::marker::Destruct + [const] CheckedMath;
    /// The maximum value which is semantically valid. Note this MUST be lower than Inner::MAX
    const MAX: Self;

    /// A 0 of the wrapped type
    const ZERO: Self;

    /// Return the underlying inner representation.
    ///
    /// For clarity it is recommended to use [cast][Self::cast] even when casting to the inner type.
    fn into_inner(self) -> Self::Inner;

    /// Cast to any integer type which the inner representation allows without risk.
    fn cast<N>(self) -> N
    where
        N: [const] From<Self::Inner>,
    {
        N::from(self.into_inner())
    }

    /// Validate Self is within upper bounds
    fn is_valid(&self) -> bool {
        self <= &Self::MAX
    }

    /// Validate Self and return `None` if invalid
    fn validate(self) -> Option<Self> {
        self.is_valid().then_some(self)
    }

    /// Validate Self and return [`Self::MAX`] if invalid
    fn assert_valid(self) -> Self {
        assert!(self.is_valid(), "semantic overflow");
        self
    }

    /// Replace invalid values with MAX. No need to saturate at `0` as this is covered by internal
    /// representation / not-(yet)-implemented trait LowerBounded.
    fn saturate(self) -> Self {
        match self.is_valid() {
            true => self,
            false => Self::MAX,
        }
    }

    #[expect(
        unsafe_code,
        reason = "need new_unchecked to allow default impls for new_*"
    )]
    /// Create a new value without validating bounds.
    ///
    /// # SAFETY
    /// The caller must ensure that the bounds are valid.
    unsafe fn new_unchecked(inner: Self::Inner) -> Self;

    /// Creates a new value from any integer type, returning `None` if the value is above
    /// [`Self::MAX`].
    ///
    /// # Note
    /// - For a panicking constructor use [`Self::strict_new`] directly, not `Self::new().unwrap()`
    /// - For a saturating constructor us [`Self::saturating_new`]
    fn new<N>(value: N) -> Option<Self>
    where
        Self::Inner: [const] CheckedCastFromInt<N>,
    {
        let inner = Self::Inner::checked_cast_from(value)?;
        #[expect(unsafe_code, reason = "default impl")]
        // SAFETY: We validate immediately
        unsafe { Self::new_unchecked(inner) }.validate()
    }

    /// Creates a new value from any integer type, panicking if the value is above [`Self::MAX`].
    ///
    /// # Note
    /// - For a non-panicking constructor use [`Self::new`] or [`Self::saturating_new`]
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented. (See [`Self::MAX`])
    fn strict_new<N>(value: N) -> Self
    where
        Self::Inner: [const] CheckedCastFromInt<N>,
    {
        let inner = Self::Inner::strict_cast_from(value);
        #[expect(unsafe_code, reason = "default impl")]
        // SAFETY: We assert validity immediately
        unsafe { Self::new_unchecked(inner) }.assert_valid()
    }

    /// Creates a new value from any integer type, replacing any values above [`Self::MAX`] with [`Self::MAX`].
    ///
    /// # Note
    /// - For a validating constructor use [`Frame::new`] or for a panicking constructor
    ///   use [`Frame::strict_new`]
    fn saturating_new<N>(frames: N) -> Self
    where
        Self::Inner: [const] BoundedCastFromInt<N>,
    {
        let inner = Self::Inner::saturating_cast_from(frames);
        #[expect(unsafe_code, reason = "default impl")]
        // SAFETY: We saturate immediately
        unsafe { Self::new_unchecked(inner) }.saturate()
    }

    /// Strict addition. Computes `self + rhs`, panicking if the result is above [`Self::MAX`].
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented. (See [`Self::MAX`])
    fn strict_add<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).into_inner();
        let v = self.into_inner().strict_add(rhs);
        Self::strict_new(v)
    }

    /// Checked addition. Computes `self + rhs`, returning `None` if the result is above
    /// [`Self::MAX`].
    fn checked_add<R>(self, rhs: R) -> Option<Self>
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).into_inner();
        let v = self.into_inner().checked_add(rhs)?;
        Self::new(v)
    }

    /// Saturating addition. Computes `self + rhs`, saturating at [`Self::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    fn saturating_add<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).into_inner();
        let v = self.into_inner().saturating_add(rhs);
        Self::saturating_new(v)
    }

    /// Strict subtraction. Computes `self - rhs`, panicking if `rhs` is greater than `self`.
    ///
    /// # Panics
    /// This function will always panic on underflow, regardless of whether overflow checks are enabled.
    fn strict_sub<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).into_inner();
        let v = self.into_inner().strict_sub(rhs);
        Self::strict_new(v)
    }

    /// Checked subtraction. Computes `self - rhs`, returning `None` if `rhs` is greater than `self`.
    fn checked_sub<R>(self, rhs: R) -> Option<Self>
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).into_inner();
        let v = self.into_inner().checked_sub(rhs)?;
        Self::new(v)
    }

    /// Saturating subtraction. Computes `self - rhs`, saturating at zero.
    fn saturating_sub<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).into_inner();
        let v = self.into_inner().saturating_sub(rhs);
        Self::saturating_new(v)
    }

    /// Strict multiplication. Computes `self * rhs`, panicking if the result is above
    /// [`Self::MAX`].
    ///
    /// # Note
    /// - `rhs` may be any scalar integer.
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is negative or [`Self::MAX`]
    fn strict_mul<N>(self, rhs: N) -> Self
    where
        Self::Inner: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in Self and self non-zero, then product is too large
        let rhs = if self == Self::ZERO {
            Self::ZERO
        } else {
            Self::strict_new(rhs)
        };
        let v = self.into_inner().strict_mul(rhs.into_inner());
        Self::strict_new::<Self::Inner>(v)
    }

    /// Checked multiplication. Computes `self * rhs`, returning `None` if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may be any scalar integer.
    fn checked_mul<N>(self, rhs: N) -> Option<Self>
    where
        Self::Inner: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in Self and self non-zero, then product is too large
        let rhs = if self == Self::ZERO {
            Self::ZERO
        } else {
            Self::strict_new(rhs)
        };
        let v = self.into_inner().checked_mul(rhs.into_inner())?;
        Self::new::<Self::Inner>(v)
    }

    /// Saturating multiplication. Computes `self * rhs`, saturating at [`Self::MAX`].
    ///
    /// `rhs` may be any scalar `N` integer
    fn saturating_mul<N>(self, rhs: N) -> Self
    where
        Self::Inner: [const] BoundedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in Self and self non-zero, then product is too large
        let rhs = if self == Self::ZERO {
            Self::ZERO
        } else {
            Self::saturating_new(rhs)
        };
        let v = self.into_inner().saturating_mul(rhs.into_inner());
        Self::saturating_new::<Self::Inner>(v)
    }

    /// Basic division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_div & strict_div, as self is valid and the result of
    ///   divison must always be 0..=self
    fn div<N>(self, rhs: N) -> Self
    where
        Self::Inner: [const] BoundedCastFromInt<N>,
    {
        // If rhs > Frame::MAX and self is valid (<= Frame::MAX) then result of div will be 0 anyway
        let rhs = Self::Inner::saturating_cast_from(rhs);
        let v = self.into_inner().div(rhs);
        Self::saturating_new::<Self::Inner>(v)
    }

    /// Checked division. Computes `self / rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be any scalar integer.
    ///
    /// # Note
    /// - This is the only specialised division. Use `div` if you can accept a panic on div by zero.
    fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        Self::Inner: [const] BoundedCastFromInt<N>,
    {
        let rhs = Self::Inner::saturating_cast_from(rhs);
        let v = self.into_inner().checked_div(rhs)?;
        Self::new(v)
    }

    /// Basic remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_rem & strict_rem, as self is valid and the remainder from
    ///   divison must always be 0..=self
    fn rem<N>(self, rhs: N) -> Self
    where
        Self::Inner: [const] BoundedCastFromInt<N>,
    {
        // If rhs > Frame::MAX and self is valid (<= Frame::MAX) then result of rem will be self anyway
        let rhs = Self::Inner::saturating_cast_from(rhs);
        let v = self.into_inner().rem(rhs);
        Self::saturating_new::<Self::Inner>(v)
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be any scalar integer.
    ///
    /// # Note
    /// - This is the only specialised remainder. Use `rem` if you can accept a panic on div by zero.
    fn checked_rem<N>(self, rhs: N) -> Option<Self>
    where
        Self::Inner: [const] BoundedCastFromInt<N>,
    {
        let rhs = Self::Inner::saturating_cast_from(rhs);
        let v = self.into_inner().checked_rem(rhs)?;
        Self::new(v)
    }
}

// ===============================================
// Frame & Msf - const interconversion & comparison
// ===============================================

const impl From<Msf> for Frame {
    fn from(msf: Msf) -> Self {
        let mins = u32::from(msf.min).saturating_mul(60).saturating_mul(75);
        let secs = u32::from(msf.sec).saturating_mul(75);
        let frames = u32::from(msf.frame);
        Self(mins.saturating_add(secs).saturating_add(frames))
    }
}

const impl From<&Msf> for Frame {
    fn from(msf: &Msf) -> Self {
        Self::from(*msf)
    }
}

const impl From<Frame> for Msf {
    /// Converts a [`Frame`] into an [`Msf`], saturating each component at `0xFF`.
    ///
    /// Normal frames (up to `Msf(0xFF, 59, 74)`) convert with `sec <= 59` and `frame <= 74`.
    /// Degenerate frames above that first saturate `sec` (`0x60..=0xFF`) while keeping
    /// `frame` sane (`0..=0x4A`), then saturate `frame` (`0x4B..=0xFF`), so that
    /// `Msf -> Frame -> Msf` round-trips for every representable [`Frame`] (including
    /// [`Frame::MAX`], which maps to `Msf(0xFF, 0xFF, 0xFF)`).
    fn from(frames: Frame) -> Self {
        let mut remaining = frames.cast::<u32>();
        let mut min = remaining / (60 * 75);
        if min > 0xFF {
            min = 0xFF;
        }
        remaining -= min * (60 * 75);
        let mut sec = remaining / 75;
        if sec > 0xFF {
            sec = 0xFF;
        }
        remaining -= sec * 75;
        let frame = if remaining > 0xFF { 0xFF } else { remaining };
        Self {
            min: min as u8,
            sec: sec as u8,
            frame: frame as u8,
        }
    }
}

const impl Ord for Frame {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

const impl PartialOrd for Frame {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const impl PartialOrd<Msf> for Frame {
    fn partial_cmp(&self, msf: &Msf) -> Option<std::cmp::Ordering> {
        self.partial_cmp(&Self::from(msf))
    }
}

const impl PartialOrd<Frame> for Msf {
    fn partial_cmp(&self, frame: &Frame) -> Option<std::cmp::Ordering> {
        Frame::from(self).partial_cmp(frame)
    }
}

const impl Eq for Frame {}

const impl PartialEq for Frame {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

const impl PartialEq<Msf> for Frame {
    fn eq(&self, msf: &Msf) -> bool {
        *self == Self::from(*msf)
    }
}

const impl PartialEq<Frame> for Msf {
    fn eq(&self, frame: &Frame) -> bool {
        Frame::from(*self) == *frame
    }
}

// ===================================
// Inter-conversion with foreign types
// ===================================

impl PartialEq<cdtoc::Duration> for Frame {
    fn eq(&self, duration: &cdtoc::Duration) -> bool {
        let (s, f) = duration.seconds_frames();
        self.cast::<u64>() == s.strict_mul(75).strict_add(f.strict_cast())
    }
}

/// This is the format returned by cdtoc for [Track::msf()][cdtoc::Track::msf]
impl TryFrom<(u32, u8, u8)> for Msf {
    type Error = TryFromIntError;

    /// Converts a `(min, sec, frame)` tuple into an [`Msf`], failing if `min` does not fit in
    /// the `u8` used to store minutes on a disc.
    fn try_from(msf: (u32, u8, u8)) -> Result<Self, Self::Error> {
        let (min, sec, frame) = msf;
        Ok(Self {
            min: u8::try_from(min)?,
            sec,
            frame,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The last "normal" MSF frame count: `Msf(0xFF, 59, 74)` (`min` at max, `sec<=59`, `frame<=74`).
    const NORMAL_MAX: u32 = 0xFF * 60 * 75 + 59 * 75 + 74;

    /// Shorthand for constructing a known-valid `Frame` constant in tests.
    fn f(n: u32) -> Frame {
        Frame::strict_new(n)
    }

    #[test]
    fn leadin_conversion() {
        let msf = Msf {
            min: 0,
            sec: 2,
            frame: 0,
        };
        let frames = LEADIN;

        assert_eq!(Msf::from(frames), msf);
        assert_eq!(Frame::from(msf), frames);
    }

    #[test]
    fn leadin_compensation() {
        let starting_frames = f(183);
        let starting_time = Msf::new(0, 2, 33);
        assert_eq!(starting_frames.relative_to_leadin(), f(33));
        assert_eq!(starting_time.relative_to_leadin(), Msf::new(0, 0, 33));
    }

    #[test]
    fn strict_new_is_const_and_validated() {
        const LEADIN_FRAMES: Frame = Frame::strict_new(150_u32);
        const MAX_OK: Frame = Frame::strict_new(Frame::MAX.0);
        assert_eq!(LEADIN_FRAMES, LEADIN);
        assert_eq!(MAX_OK, Frame::MAX);
    }

    #[test]
    fn strict_new_accepts_any_u32_convertible_scalar() {
        assert_eq!(Frame::strict_new(150_u8), LEADIN);
        assert_eq!(Frame::strict_new(150_u16), LEADIN);
        assert_eq!(Frame::strict_new(150_u32), LEADIN);
        assert_eq!(Frame::strict_new(Frame::MAX.0), Frame::MAX);
    }

    #[test]
    #[should_panic(expected = "semantic overflow")]
    fn strict_new_panics_above_max() {
        let _: Frame = Frame::strict_new(Frame::MAX.0 + 1);
    }

    #[test]
    fn new_is_fallible_and_validated() {
        // valid values -> Some
        assert_eq!(Frame::new(0_u32), Some(f(0)));
        assert_eq!(Frame::new(150_usize), Some(LEADIN));
        assert_eq!(Frame::new(Frame::MAX.0), Some(Frame::MAX));
        assert_eq!(Frame::new(u64::from(Frame::MAX.0)), Some(Frame::MAX));
        // above MAX -> None
        assert_eq!(Frame::new(Frame::MAX.0 + 1), None);
        // does not fit in a u32 -> None
        assert_eq!(Frame::new(u64::from(u32::MAX) + 1), None);
        assert_eq!(Frame::new(-1_i32), None);
    }

    #[test]
    fn frame_add_sub_rem_flavours() {
        // add: rhs may be Frame or Msf
        assert_eq!(f(1).strict_add(f(2)), f(3));
        assert_eq!(f(1).strict_add(Msf::new(0, 0, 2)), f(3));
        assert_eq!(Frame::MAX.checked_add(f(1)), None);
        assert_eq!(f(1).checked_add(Msf::new(0, 0, 1)), Some(f(2)));
        assert_eq!(Frame::MAX.saturating_add(f(1)), Frame::MAX);

        // sub
        assert_eq!(f(5).strict_sub(f(3)), f(2));
        assert_eq!(f(5).strict_sub(Msf::new(0, 0, 3)), f(2));
        assert_eq!(f(1).checked_sub(f(2)), None);
        assert_eq!(f(1).saturating_sub(f(2)), f(0));

        // rem
        // assert_eq!(f(150).strict_rem(Msf::new(0, 0, 7)), f(3));
        // assert_eq!(f(150).checked_rem(f(0)), None);
        // assert_eq!(f(150).wrapping_rem(f(7)), f(3));
    }

    #[test]
    fn frame_mul_div_flavours_take_scalars_only() {
        assert_eq!(f(2).strict_mul(75_u8), f(150));
        assert_eq!(f(2).strict_mul(75_u16), f(150));
        // assert_eq!(f(150).strict_div(75_u32), f(2));
        assert_eq!(Frame::MAX.checked_mul(2_u32), None);
        assert_eq!(Frame::MAX.saturating_mul(2_u32), Frame::MAX);
        assert_eq!(f(150).checked_div(0_u32), None);
        // assert_eq!(f(150).saturating_div(75_u8), f(2));
        // assert_eq!(f(150).wrapping_div(75_u8), f(2));
    }

    #[test]
    #[should_panic(expected = "semantic overflow")]
    fn strict_add_panics_above_max() {
        let _: Frame = Frame::MAX.strict_add(f(1));
    }

    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn strict_sub_panics_on_underflow() {
        let _: Frame = f(1).strict_sub(f(2));
    }

    // #[test]
    // #[should_panic(expected = "attempt to divide by zero")]
    // fn strict_div_panics_on_zero() {
    //     let _: Frame = f(1).strict_div(0_u32);
    // }

    #[test]
    fn msf_arithmetic_is_computed_via_frame() {
        assert_eq!(Msf::new(0, 2, 0).strict_add(LEADIN), Msf::new(0, 4, 0));
        assert_eq!(Msf::new(0, 2, 0).strict_add(f(150)), Msf::new(0, 4, 0));
        assert_eq!(
            Msf::new(0, 4, 0).strict_sub(Msf::new(0, 2, 0)),
            Msf::new(0, 2, 0)
        );
        assert_eq!(Msf::new(0, 0, 33).saturating_sub(LEADIN), Msf::new(0, 0, 0));
        assert_eq!(Msf::new(0, 0, 33).checked_sub(LEADIN), None);
        // results are normalised via Frame
        assert_eq!(Msf::new(1, 60, 75).strict_add(f(0)), Msf::new(2, 1, 0));
        // mul / div take a u8 scalar
        assert_eq!(Msf::new(1, 0, 0).strict_mul(2_u8), Msf::new(2, 0, 0));
        // assert_eq!(Msf::new(2, 0, 0).strict_div(2_u8), Msf::new(1, 0, 0));
        // assert_eq!(
        //     Msf::new(0, 2, 33).strict_rem(Msf::new(0, 0, 7)),
        //     Msf::new(0, 0, 1)
        // );
        assert_eq!(
            Msf::new(1, 0, 0).checked_mul(255_u8),
            Some(Msf::new(0xFF, 0, 0))
        ); // 4500 * 255 = 1_147_500
        assert_eq!(Msf::new(0xFF, 0xFF, 0xFF).checked_mul(2_u8), None); // Frame::MAX * 2 is over the max
    }

    #[test]
    fn msf_try_from_tuple() {
        assert_eq!(
            Msf::try_from((2_u32, 34_u8, 13_u8)).unwrap(),
            Msf::new(2, 34, 13)
        );
        assert!(Msf::try_from((256_u32, 0_u8, 0_u8)).is_err());
        assert!(Msf::try_from((u32::from(u8::MAX), 0_u8, 0_u8)).is_ok());
    }

    // ============================================
    // Round-trip invariant: Msf -> Frame -> Msf
    // ============================================

    #[test]
    fn msf_frame_msf_roundtrips_required_cases() {
        for msf in [
            Msf::new(0, 0, 0),
            Msf::new(0, 2, 0),
            Msf::new(0, 2, 33),
            Msf::new(0xFF, 0xFF, 0xFF),
        ] {
            let frame = Frame::from(msf);
            assert!(frame.cast::<u32>() <= Frame::MAX.0);
            assert_eq!(Msf::from(frame), msf, "roundtrip failed for {msf:?}");
        }
    }

    #[test]
    fn msf_frame_msf_roundtrips_degenerate_and_near_degenerate() {
        let cases = [
            Msf::new(0xFF, 59, 74), // last normal value (0xFF, 0d59, 0d74)
            Msf::new(0xFF, 60, 0),  // first degenerate: secs saturate from 0d60
            Msf::new(0xFF, 60, 74), // secs saturated, frames still sane
            Msf::new(0xFF, 0xFE, 74),
            Msf::new(0xFF, 0xFF, 0), // sec fully saturated
            Msf::new(0xFF, 0xFF, 74),
            Msf::new(0xFF, 0xFF, 75), // frames now saturate 0d75..=0xFF
            Msf::new(0xFF, 0xFF, 0xFE),
            Msf::new(0xFF, 0xFF, 0xFF), // Frame::MAX
        ];
        for msf in cases {
            let frame = Frame::from(msf);
            assert_eq!(
                Msf::from(frame),
                msf,
                "degenerate roundtrip failed for {msf:?}"
            );
        }
    }

    #[test]
    fn frame_msf_frame_roundtrips_exhaustively() {
        // Frame -> Msf -> Frame must hold for every representable frame.
        // Normal range keeps sec<=59 and frame<=74; degenerate range saturates min at 0xFF.
        let mut f: u32 = 0;
        while f <= Frame::MAX.0 {
            let frame = Frame::strict_new(f);
            let msf = Msf::from(frame);
            assert_eq!(Frame::from(msf), frame, "Frame->Msf->Frame failed at {f}");
            if f <= NORMAL_MAX {
                assert!(msf.sec <= 59, "sec too large at {f}: {msf:?}");
                assert!(msf.frame <= 74, "frame too large at {f}: {msf:?}");
            } else {
                assert_eq!(msf.min, 0xFF, "min not saturated at {f}: {msf:?}");
            }
            f += 1;
        }
    }

    #[test]
    fn degenerate_frame_to_msf_saturates_secs_then_frames() {
        // just above normal -> secs start at 0d60, frames sane
        let msf = Msf::from(Frame::strict_new(NORMAL_MAX + 1));
        assert_eq!(msf, Msf::new(0xFF, 60, 0));
        // sec fully saturated, frame still sane (0..=0d74)
        let msf = Msf::from(Frame::strict_new(0xFF_u32 * 60 * 75 + 0xFF * 75 + 74));
        assert_eq!(msf, Msf::new(0xFF, 0xFF, 74));
        // then frames saturate 0d75..=0xFF
        let msf = Msf::from(Frame::strict_new(0xFF_u32 * 60 * 75 + 0xFF * 75 + 75));
        assert_eq!(msf, Msf::new(0xFF, 0xFF, 75));
        let msf = Msf::from(Frame::MAX);
        assert_eq!(msf, Msf::new(0xFF, 0xFF, 0xFF));
    }
}
