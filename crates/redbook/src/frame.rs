//! Time units: Audio CDs measure time in 1/75 second "frames". Provides `Frame` & `Msf`
//! (min, sec, frame), plus `ByteSize` for sizes in bytes.

use std::{
    convert::{BoundedCastFromInt, CheckedCastFromInt},
    fmt::{Debug, Display},
    marker::{Destruct, StructuralPartialEq},
    num::{ParseIntError, TryFromIntError},
    ops::{Div, Rem},
    str::FromStr,
};

use crate::LEADIN;

/// CD audio frame (1/75 sec). Basic unit of time for CD audio.
///
/// A frame represents a single unit of CD audio data, which is 1/75th of a second.
/// This is the fundamental unit of time measurement for CD audio.
///
/// # Construction & Conversion
///
/// ```
/// # use redbook::Frame;
/// // checked construction returns `None` for invalid values
/// let frame = Frame::new(2000).unwrap_or(Frame::ZERO);
///
/// // by convention frames represent absolute position, including the 150-frame lead-in
/// let relative = frame.relative_to_leadin();
///
/// // .cast() can be used to convert to a primitive integer
/// assert_eq!(relative.cast::<u32>(), 1850);
/// ```
///
/// # Invariant
///
/// A `Frame` can always be converted into an [`Msf`] representation: the stored value is always
/// `<=` [`Frame::MAX`]. This is upheld by validating construction ([`Frame::new`],
/// [`Frame::strict_new`] & [`Frame::saturating_new`]) and by every arithmetic method below, so
/// that `From<Frame> for Msf` is always well-defined.
///
/// While the primary constructors are designed to validate the upper bound, unsafe code MUST NOT
/// rely on this.
///
/// # Arithmetic
///
/// `+`, `-`, `*`, `/`, `%`, `+=` etc. are deliberately not implemented for `Frame` (or [`Msf`]).
/// Instead use the `saturating_*`, `checked_` & `strict_*` methods, which accept only
/// `rhs: Frame` or `rhs: Msf` for add/sub, and only a scalar integer `N` for mul/div/rem.
///
/// ```
/// # use redbook::Frame;
/// let frame = Frame::new(2000).unwrap();
/// let not_negative = frame.checked_sub(Frame::MAX);
/// assert!(not_negative.is_none());
///
/// let super_max = Frame::MAX.saturating_add(Frame::MAX);
/// assert_eq!(super_max, Frame::MAX);
/// ```
///
/// # Obtain the required buffer size to store this many `Frame`s
///
/// To convert a frame count into a byte count use [`Frame::size`], which returns a
/// [`ByteSize`]. [`Frame::SIZE_IN_BYTES`] is itself a [`ByteSize`], so it cannot be used as the
/// scalar `rhs` of the mul methods: a `Frame` times a byte size is a `ByteSize`, never a
/// `Frame`.
///
/// ```
/// # use redbook::{frame::ByteSize, Frame};
/// let frames = Frame::new(1).unwrap();
/// // how many bytes are needed?
/// assert_eq!(frames.size(), ByteSize::strict_new(2352));
///
/// // On 32-bit or 64-bit architecture Frame::MAX.size() fits in a usize
/// let buf = vec![0_u8; frames.size().strict_cast()];
///
/// // One frame needs 2352 bytes in raw CDDA mode (excluding sub-channels)
/// assert_eq!(Frame::SIZE_IN_BYTES, ByteSize::strict_new(2352));
/// assert_eq!(buf.len(), 2352);
/// ```
///
/// # Pattern matching
///
/// [`Frame`] implements [`StructuralPartialEq`], so constants such as [`Frame::ZERO`],
/// [`Frame::ONE_SECOND`] or [`LEADIN`] can be used directly as `match` arms:
/// there is no need to [`cast`][Frame::cast] to a primitive int first.
///
/// ```
/// use redbook::{Frame, LEADIN};
///
/// let frame = Frame::strict_new(150);
/// let description = match frame {
///     Frame::ZERO => "start of the disc",
///     LEADIN => "start of the program area",
///     _ => "somewhere on the disc",
/// };
/// assert_eq!(description, "start of the program area");
/// ```
///
/// Range patterns are not available on `Frame` itself, so matches on ranges still go via a
/// primitive int:
///
/// ```
/// use redbook::{Frame, LEADIN};
///
/// let frame = Frame::strict_new(100);
/// let in_lead_in = match frame.cast::<u32>() {
///     0..=149 => true, // 0..=LEADIN as a primitive
///     _ => false,
/// };
/// assert!(in_lead_in);
/// ```
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct Frame(
    /// Internally frames are stored as u32. The maximum frame size in a real-world situation
    /// is limited by the (u8, u8, u8) representation of MSF when stored on a disc. Given that
    /// (0xFF * 60 * 75) = 0x11826C, 3 bytes are needed to store 0xFF minutes, so a u16 is too small
    /// and anything over u32 is oversized.
    u32,
);

/// TODO: #124 currently outputs "Frame n" should probably just be "n", or 2 types: position & duration
impl Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame {}", self.0)
    }
}

impl Frame {
    /// The maximum frame value which can be represented in a TOC or other sub-channel data
    /// which stores times as MSF(u8,u8,u8).
    ///
    /// Given [constant improvements in manufacturing][wiki], this is certainly slightly
    /// oversized, but some crazy person may choose to ignore that there are only 60 seconds in
    /// a minute and store an MSF of (FF, FF, FF) for some reason.
    ///
    /// [wiki]: https://en.wikipedia.org/wiki/Compact_Disc_Digital_Audio#Storage_capacity_and_playing_time
    pub const MAX: Self = Frame((0xFF * 60 * 75) + (0xFF * 75) + 0xFF);

    /// A zero [`Frame`]
    pub const ZERO: Self = Frame(0);

    /// One second, i.e. 75 frames (a frame is 1/75th of a second).
    pub const ONE_SECOND: Self = Frame(75);

    /// One minute, i.e. 60 seconds.
    pub const ONE_MINUTE: Self = Frame(Self::ONE_SECOND.into_inner() * 60);

    /// Size of a single CDDA audio frame in bytes.
    ///
    /// According to the RedBook standard (IEC 60908:1999), each frame contains 2352 bytes
    /// of raw audio data.
    ///
    /// # Note
    /// - This is a [`ByteSize`], not a scalar, so it cannot be passed as the `rhs` of the
    ///   `strict_mul` & friends: `frame.strict_mul(Frame::SIZE_IN_BYTES)` does not compile.
    ///   Use [`Frame::size`] to get the byte size of a frame count instead.
    pub const SIZE_IN_BYTES: ByteSize = ByteSize(2352);

    /// Creates a new `Frame` from any integer type, returning `None` if the value is above
    /// [`Self::MAX`].
    ///
    /// # Note
    /// - For a panicking constructor use [`Self::strict_new`] directly, not `Self::new().unwrap()`
    /// - For a saturating constructor use [`Self::saturating_new`]
    pub const fn new<N>(value: N) -> Option<Self>
    where
        u32: [const] CheckedCastFromInt<N>,
    {
        let inner = u32::checked_cast_from(value)?;
        Self(inner).validate()
    }

    /// Creates a new `Frame` from any integer type, panicking if the value is above [`Self::MAX`].
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
    pub const fn strict_new<N>(value: N) -> Self
    where
        u32: [const] CheckedCastFromInt<N>,
    {
        let inner = u32::strict_cast_from(value);
        Self(inner).assert_valid()
    }

    /// Creates a new `Frame` from any integer type, replacing any values above [`Self::MAX`]
    /// with [`Self::MAX`].
    ///
    /// # Note
    /// - For a validating constructor use [`Self::new`] or for a panicking constructor
    ///   use [`Self::strict_new`]
    pub const fn saturating_new<N>(value: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let inner = u32::saturating_cast_from(value);
        Self(inner).saturate()
    }

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

    /// Returns the underlying `u32` representation.
    ///
    /// For clarity it is recommended to use [`Self::cast`] even when casting to `u32`.
    const fn into_inner(self) -> u32 {
        self.0
    }

    /// Casts to any integer type which the inner representation allows without risk.
    pub const fn cast<N>(self) -> N
    where
        N: [const] From<u32>,
    {
        N::from(self.into_inner())
    }

    /// Casts to any integer type returning `None` on overflow.
    pub const fn checked_cast<N>(self) -> Option<N>
    where
        N: [const] CheckedCastFromInt<u32>,
    {
        N::checked_cast_from(self.into_inner())
    }

    /// Casts to any integer type saturating on overflow.
    pub const fn saturating_cast<N>(self) -> N
    where
        N: [const] BoundedCastFromInt<u32>,
    {
        N::saturating_cast_from(self.into_inner())
    }

    /// Casts to any integer type panicking on overflow.
    pub const fn strict_cast<N>(self) -> N
    where
        N: [const] CheckedCastFromInt<u32>,
    {
        N::strict_cast_from(self.into_inner())
    }

    /// The size of `self` in bytes: `self * `[`Self::SIZE_IN_BYTES`].
    ///
    /// This is the type-safe way to turn a frame count into a byte count: the result is a
    /// [`ByteSize`], so it cannot be mistaken for another `Frame` (position or duration).
    ///
    /// # Examples
    /// ```
    /// use redbook::{Frame, frame::ByteSize};
    ///
    /// let one_second = Frame::ONE_SECOND;
    /// assert_eq!(one_second.size(), ByteSize::strict_new(75 * 2352));
    /// assert_eq!(one_second.size().frames_exact(), one_second);
    /// ```
    pub const fn size(self) -> ByteSize {
        // Direct construction: strict_new causes a MIR-opt cycle for ByteSize::MAX
        // Cannot panic due to Invariant & ability to compile ByteSize::MAX
        ByteSize(
            self.strict_cast::<usize>()
                .strict_mul(Self::SIZE_IN_BYTES.cast()),
        )
    }

    /// Validates `self` is within upper bounds
    pub const fn is_valid(&self) -> bool {
        self <= &Self::MAX
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

    /// Replaces invalid values with [`Self::MAX`]. No need to saturate at `0` as this is covered
    /// by the internal representation.
    pub const fn saturate(self) -> Self {
        match self.is_valid() {
            true => self,
            false => Self::MAX,
        }
    }

    // ============================
    // Arithmetic: add / sub
    //
    // rhs is restricted by From
    // which only allows Frame & Msf
    // ============================

    /// Strict addition. Computes `self + rhs`, panicking if the result is above [`Self::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented. (See [`Self::MAX`])
    pub const fn strict_add<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Frame::from(rhs).into_inner();
        let v = self.into_inner().strict_add(rhs);
        Self::strict_new::<u32>(v)
    }

    /// Checked addition. Computes `self + rhs`, returning `None` if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_add<R>(self, rhs: R) -> Option<Self>
    where
        Self: [const] From<R>,
    {
        let rhs = Frame::from(rhs).into_inner();
        let v = self.into_inner().checked_add(rhs)?;
        Self::new::<u32>(v)
    }

    /// Saturating addition. Computes `self + rhs`, saturating at [`Self::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_add<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Frame::from(rhs).into_inner();
        let v = self.into_inner().saturating_add(rhs);
        Self::saturating_new::<u32>(v)
    }

    /// Strict subtraction. Computes `self - rhs`, panicking if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// This function will always panic on underflow, regardless of whether overflow checks are enabled.
    pub const fn strict_sub<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Frame::from(rhs).into_inner();
        let v = self.into_inner().strict_sub(rhs);
        Self::strict_new::<u32>(v)
    }

    /// Checked subtraction. Computes `self - rhs`, returning `None` if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_sub<R>(self, rhs: R) -> Option<Self>
    where
        Self: [const] From<R>,
    {
        let rhs = Frame::from(rhs).into_inner();
        let v = self.into_inner().checked_sub(rhs)?;
        Self::new::<u32>(v)
    }

    /// Saturating subtraction. Computes `self - rhs`, saturating at zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_sub<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Frame::from(rhs).into_inner();
        let v = self.into_inner().saturating_sub(rhs);
        Self::saturating_new::<u32>(v)
    }

    // ============================
    // Arithmetic: mul
    //
    // rhs may only be a scalar N
    // deliberately not Frame/Msf as
    // we don't expect acceleration
    // calculations
    // ============================

    /// Strict multiplication. Computes `self * rhs`, panicking if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may be any scalar integer.
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented. (See [`Self::MAX`])
    pub const fn strict_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in a Frame and self is non-zero, then the product is too large
        let rhs = match self {
            Self::ZERO => Self::ZERO,
            _ => Self::strict_new(rhs),
        };
        let v = self.into_inner().strict_mul(rhs.into_inner());
        Self::strict_new::<u32>(v)
    }

    /// Checked multiplication. Computes `self * rhs`, returning `None` if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may be any scalar integer.
    pub const fn checked_mul<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in a Frame and self is non-zero, then the product is too large
        let rhs = match self {
            Self::ZERO => Self::ZERO,
            _ => Self::new(rhs)?,
        };
        let v = self.into_inner().checked_mul(rhs.into_inner())?;
        Self::new::<u32>(v)
    }

    /// Saturating multiplication. Computes `self * rhs`, saturating at [`Self::MAX`].
    ///
    /// `rhs` may be any scalar `N` integer
    pub const fn saturating_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in a Frame and self is non-zero, then the product is too large
        let rhs = match self {
            Self::ZERO => Self::ZERO,
            _ => Self::saturating_new(rhs),
        };
        let v = self.into_inner().saturating_mul(rhs.into_inner());
        Self::saturating_new::<u32>(v)
    }

    // ============================
    // Arithmetic: div / rem
    // ============================

    /// Basic division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_div & strict_div, as self is valid and the result of
    ///   division must always be 0..=self
    ///
    /// # Panics
    /// Panics if `rhs` is zero. The cast clamps `rhs` into `u32` range first: a negative `rhs`
    /// becomes 0 (and so also panics), while an `rhs` above `u32::MAX` becomes `u32::MAX` —
    /// larger than any valid `Frame`, so the result is [`Frame::ZERO`].
    pub const fn div<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let rhs = u32::saturating_cast_from(rhs);
        let v = self.into_inner().div(rhs);
        Self::saturating_new::<u32>(v)
    }

    /// Checked division. Computes `self / rhs`, returning `None` if `rhs` clamps to zero
    /// (i.e. `rhs` is zero or negative).
    ///
    /// `rhs` may be any scalar integer. An `rhs` above `u32::MAX` clamps to `u32::MAX`, which is
    /// larger than any valid `Frame`, yielding `Some(Frame::ZERO)`.
    ///
    /// # Note
    /// - This is the only specialised division. Use `div` if you can accept a panic on div by zero.
    pub const fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let rhs = u32::saturating_cast_from(rhs);
        let v = self.into_inner().checked_div(rhs)?;
        Self::new::<u32>(v)
    }

    /// Basic remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_rem & strict_rem, as self is valid and the remainder from
    ///   division must always be 0..=self
    ///
    /// # Panics
    /// Panics if `rhs` is zero. The cast clamps `rhs` into `u32` range first: a negative `rhs`
    /// becomes 0 (and so also panics), while an `rhs` above `u32::MAX` becomes `u32::MAX` —
    /// larger than any valid `Frame`, so the result is `self`.
    pub const fn rem<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let rhs = u32::saturating_cast_from(rhs);
        let v = self.into_inner().rem(rhs);
        Self::saturating_new::<u32>(v)
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` if `rhs` clamps to zero
    /// (i.e. `rhs` is zero or negative).
    ///
    /// `rhs` may be any scalar integer. An `rhs` above `u32::MAX` clamps to `u32::MAX`, which is
    /// larger than any valid `Frame`, yielding `Some(self)`.
    ///
    /// # Note
    /// - This is the only specialised remainder. Use `rem` if you can accept a panic on div by zero.
    pub const fn checked_rem<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let rhs = u32::saturating_cast_from(rhs);
        let v = self.into_inner().checked_rem(rhs)?;
        Self::new::<u32>(v)
    }

    /// Saturating division & remainder. Returns `(quotient, rest)`, where `quotient` is
    /// `self / rhs` saturated into `D` (e.g. `u8` for an [`Msf`] component) and `rest` is the
    /// leftover frames.
    ///
    /// Unlike [`Self::div`] & [`Self::rem`], `rest` is computed from the *saturated* quotient, so
    /// it may be `>= rhs`: the pair always satisfies `self == (quotient * rhs) + rest` and
    /// `quotient <= D::MAX`.
    ///
    /// # Example
    ///
    /// ```
    /// # use redbook::Frame;
    /// let huge = Frame::MAX; // 0xFF mins, 0xFF secs, 0xFF frames
    /// let (min, rest): (u8, Frame) = huge.saturating_div_rem(Frame::ONE_MINUTE);
    /// assert_eq!(min, 0xFF);
    /// assert_eq!(rest.cast::<u32>(), 0x4BB4);
    /// let (sec, frame) = rest.saturating_div_rem::<u8>(Frame::ONE_SECOND);
    /// assert_eq!(sec, 0xFF);
    /// assert_eq!(frame.strict_cast::<u8>(), 0xFF);
    /// ```
    ///
    /// # Panics
    /// Panics if `rhs` is [`Self::ZERO`].
    pub const fn saturating_div_rem<D>(self, rhs: Self) -> (D, Self)
    where
        D: [const] BoundedCastFromInt<u32> + Copy,
        u32: [const] CheckedCastFromInt<D>,
    {
        let rhs = rhs.cast::<u32>();
        let quotient = self.div(rhs).saturating_cast::<D>();

        // The strict_* calls below cannot panic: quotient <= self / rhs, so
        // quotient * rhs <= self <= Frame::MAX, and rest = self - quotient * rhs >= 0.
        let divided_out = Frame::strict_new(quotient).strict_mul::<u32>(rhs);
        let rest = self.strict_sub(divided_out);

        (quotient, rest)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// CD audio time in min:sec:frame format (75 frames/sec).
///
/// MSF (Minute-Second-Frame) is a common format for representing CD audio positions and is used
/// in the table of contents stored in the lead-in of every disc.
///
/// # Examples
///
/// ## Interconversion with Frame (preferred)
///
/// ```
/// use redbook::{Frame, frame::Msf};
///
/// let msf = Msf::new(1, 2, 3);
/// let frames = Frame::from(msf);
/// assert_eq!(frames, Frame::strict_new(60 * 75 + 2 * 75 + 3));
///
/// // converting back through Frame normalises degenerate components:
/// // Msf(1, 60, 75) is 2 minutes, 1 second and 0 frames
/// assert_eq!(Msf::from(frames), msf);
/// assert_eq!(
///     Msf::from(Frame::from(Msf::new(1, 60, 75))),
///     Msf::new(2, 1, 0)
/// );
/// ```
///
/// ## Direct / alternative construction & usage
///
/// ```
/// use redbook::frame::Msf;
///
/// // components are stored as-is; no validation or normalisation is performed
/// let msf = Msf::new(0, 2, 0);
/// assert_eq!(msf.as_u8s(), (0, 2, 0));
///
/// // cdtoc returns track positions as (min, sec, frame) with min as u32:
/// // `cdtoc::Track::msf()` can be converted via `Msf::try_from(track.msf())`
/// let from_cdtoc = Msf::try_from((5u32, 0u8, 2u8)).unwrap();
/// assert_eq!(from_cdtoc.as_u8s(), (5, 0, 2));
///
/// // a minute which does not fit in the u8 stored on disc is rejected
/// assert!(Msf::try_from((256u32, 0u8, 0u8)).is_err());
/// ```
///
/// # Notes
///
/// - `min` is `u8`, and therefore restricted to 256, as this is designed to only store values
///   which can be meaningfully represented on a CD. The technical format used on discs only
///   provides 1 byte for storing mins.
/// - `Msf` does not perform any validation that sec < 60 or frames < 75, or normalisation of such
///   cases. However, converting a value such as `Msf(1, 60, 75)` to [`Frame`]s and back will result
///   in the more meaningful and correct `Msf(2, 1, 0)`.
/// - Most comparison and arithmatic operations round-trip via [`Frame`] and so perform
///   normalisation as described above.
/// - Downstream implementors of conversions and users of Msf should pay attention to handling
///   technically correct but semantically invalid cases, such as the example above.
/// - The reason for the above are historical documented cases of publishers deliberately creating
///   degenerate TOC entries to confuse early PCs. In the unlikely event that a CD manufacturer
///   manages to squeeze over 255 minutes of audio onto a disc, Msf will saturate mins first,
///   then secs, then frames. E.g. 255:15/3 would be 254 (0xFF) mins, 75 secs, 3 frames
/// - It is recommended to use [`Frame`] as the primary timescale to avoid such semantic
///   concerns leading to hidden logic errors.
///
/// # Arithmetic
///
/// `+`, `-`, `*`, `/`, `%` etc. are deliberately not implemented for `Msf`. Use the
/// `saturating_*`, `checked_` & `strict_*` methods instead. These accept only
/// `rhs: Frame` or `rhs: Msf` for add/sub, and only a scalar integer `N` for mul/div/rem.
/// Arithmetic is computed via [`Frame`], so results are always normalised
/// (e.g. `Msf::new(1, 60, 75).strict_add(...)`) and any [`Frame::MAX`] bound is respected.
pub struct Msf {
    /// Minutes component.
    min: u8,
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

    /// Returns the `(min, sec, frame)` components as a tuple of `u8`s.
    ///
    /// In general it is recommended to use [`Frame`] as the primary measure of time. If you do
    /// need the individual components of an `Msf`, e.g. to decode Q-channel / TOC data, then
    /// make sure to call this function early, before any potential normalisation. (See type-level
    /// docs for more info)
    pub const fn as_u8s(self) -> (u8, u8, u8) {
        (self.min, self.sec, self.frame)
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
    /// -  "degenerate"/"exceptional" cases will saturate as described in the notes for [`Msf`]
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
    /// `rhs` may be any scalar integer `N`.
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
    /// `rhs` may be any scalar integer `N`.
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
    /// `rhs` may be any scalar integer `N`.
    pub const fn saturating_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
        N: [const] Destruct,
    {
        let lhs = Frame::from(self);
        let frames = lhs.saturating_mul(rhs);
        Self::from(frames)
    }

    /// Basic division. Computes `self / rhs`.
    ///
    /// # Note
    /// - This replaces both saturating_div & strict_div, as self is valid and the result of
    ///   divison must always be 0..=self
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::div`].
    pub const fn div<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.div(rhs);
        Self::from(frames)
    }

    /// Checked division. Computes `self / rhs`, returning `None` under the same conditions as
    /// [`Frame::checked_div`].
    ///
    /// `rhs` may be any scalar integer `N`.
    pub const fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.checked_div(rhs);
        frames.map(Into::into)
    }

    /// Basic remainder. Computes `self % rhs`.
    ///
    /// # Note
    /// - This replaces both saturating_rem & strict_rem, as self is valid and the remainder from
    ///   divison must always be 0..=self
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::rem`].
    pub const fn rem<N>(self, rhs: N) -> Self
    where
        u32: [const] BoundedCastFromInt<N>,
    {
        let lhs = Frame::from(self);
        let frames = lhs.rem(rhs);
        Self::from(frames)
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` under the same conditions as
    /// [`Frame::checked_rem`].
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

impl Display for Msf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{m}:{s}/{f}", m = self.min, s = self.sec, f = self.frame)
    }
}

// ===============================================
// Frame & Msf - const interconversion & comparison
// ===============================================

const impl From<Msf> for Frame {
    fn from(msf: Msf) -> Self {
        let mins = u32::from(msf.min).saturating_mul(Frame::ONE_MINUTE.into_inner());
        let secs = u32::from(msf.sec).saturating_mul(Frame::ONE_SECOND.into_inner());
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
        let (min, rest) = frames.saturating_div_rem::<u8>(Frame::ONE_MINUTE);
        let (sec, frame) = rest.saturating_div_rem::<u8>(Frame::ONE_SECOND);
        Self {
            min,
            sec,
            frame: frame.saturating_cast(),
        }
    }
}

const impl From<&Frame> for Msf {
    fn from(frame: &Frame) -> Self {
        Self::from(*frame)
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

impl StructuralPartialEq for Frame {}

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
        self.cast::<u64>() == s.strict_mul(75).strict_add(f.into())
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

// =====================
// ByteSize - sizes in bytes
// =====================

/// A size in bytes.
///
/// CDDA data is counted in [`Frame`]s, but buffers & files are counted in bytes. `ByteSize`
/// keeps the two apart: [`Frame::size`] multiplies a frame count by [`Frame::SIZE_IN_BYTES`]
/// and returns a `ByteSize`, not another `Frame`.
///
/// # Invariant
///
/// A `ByteSize` can always be converted into a [`Frame`] count: the stored value is always
/// `<=` [`ByteSize::MAX`] (= [`Frame::MAX`]'s size in bytes). This is upheld by validating
/// construction ([`ByteSize::new`], [`ByteSize::strict_new`] & [`ByteSize::saturating_new`])
/// and by every arithmetic method below, so that e.g. [`ByteSize::frames_floor`] is
/// always well-defined.
///
/// # Notes
/// - Internally byte sizes are stored as a `usize`, so platform-dependent values such as
///   `buf.len()` can be used directly, e.g. `ByteSize::new(buf.len())`. [`ByteSize::MAX`] is
///   defined as [`Frame::MAX`]'s size in bytes, so the crate only compiles on targets where a
///   `usize` is large enough to store every frame-derived byte size.
/// - Construct via [`ByteSize::new`], [`ByteSize::strict_new`] or
///   [`ByteSize::saturating_new`], and read the value via the `cast` methods
///   ([`ByteSize::cast`], [`ByteSize::checked_cast`], ...).
/// - `Deref` & `DerefMut` are deliberately not implemented, and neither are `+`, `-`, `*`,
///   `/`, `%`, `+=` etc. Use the `strict_*`, `checked_` & `saturating_` methods instead, which
///   accept only `rhs: ByteSize` for add/sub and only a scalar integer `N` for mul/div/rem.
///   Do NOT calculate the raw value yourself and wrap the result back in a `ByteSize`.
/// - To convert to a frame count, use [`ByteSize::frames_floor`] or
///   [`ByteSize::frames_exact`]/[`ByteSize::frames_exact_checked`], which divide by
///   [`Frame::SIZE_IN_BYTES`].
/// - `PartialEq` & `PartialOrd` are deliberately not implemented between `ByteSize` &
///   primitive ints, to ensure comparisons don't inadvertently compare a `ByteSize` with
///   something semantically different. Use [`cast()`][Self::cast] to get a primitive
///   representation.
///
/// # Examples
///
/// See [`Frame`] for a further usage example
///
/// ```
/// use redbook::{Frame, frame::ByteSize};
///
/// let bytes = Frame::strict_new(100).size();
/// assert_eq!(bytes, ByteSize::strict_new(235_200));
/// assert_eq!(bytes.frames_floor(), Frame::strict_new(100));
/// assert_eq!(bytes.frames_exact_checked(), Some(Frame::strict_new(100)));
///
/// // not a whole number of frames
/// assert_eq!(ByteSize::strict_new(2353).frames_exact_checked(), None);
/// ```
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct ByteSize(
    /// Internally byte sizes are stored as a `usize`, so that e.g. `buf.len()` can be used
    /// directly. The semantic upper bound is [`ByteSize::MAX`], which is defined as a `const`
    /// from [`Frame::MAX`]'s size: the crate therefore only compiles on targets where a
    /// `usize` is large enough to store every frame-derived byte size.
    usize,
);

impl ByteSize {
    /// The maximum byte size which can be derived from a [`Frame`]: [`Frame::MAX`]'s size in
    /// bytes.
    ///
    /// As this is a `const`, the crate only compiles on targets where a `usize` is large
    /// enough to store every possible frame-derived byte size.
    pub const MAX: Self = Frame::MAX.size();

    /// A zero [`ByteSize`]
    pub const ZERO: Self = Self(0);

    /// Creates a new `ByteSize` from any integer type, returning `None` if the value is above
    /// [`Self::MAX`].
    ///
    /// # Note
    /// - For a panicking constructor use [`Self::strict_new`] directly, not `Self::new().unwrap()`
    /// - For a saturating constructor use [`Self::saturating_new`]
    pub const fn new<N>(value: N) -> Option<Self>
    where
        usize: [const] CheckedCastFromInt<N>,
    {
        let inner = usize::checked_cast_from(value)?;
        Self(inner).validate()
    }

    /// Creates a new `ByteSize` from any integer type, panicking if the value is above
    /// [`Self::MAX`].
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
    pub const fn strict_new<N>(value: N) -> Self
    where
        usize: [const] CheckedCastFromInt<N>,
    {
        let inner = usize::strict_cast_from(value);
        Self(inner).assert_valid()
    }

    /// Creates a new `ByteSize` from any integer type, replacing any values above [`Self::MAX`]
    /// with [`Self::MAX`].
    ///
    /// # Note
    /// - For a validating constructor use [`Self::new`], or for a panicking constructor use
    ///   [`Self::strict_new`]
    pub const fn saturating_new<N>(value: N) -> Self
    where
        usize: [const] BoundedCastFromInt<N>,
    {
        let inner = usize::saturating_cast_from(value);
        Self(inner).saturate()
    }

    /// Casts to any integer type which the inner representation allows without risk.
    pub const fn cast<N>(self) -> N
    where
        N: [const] From<usize>,
    {
        N::from(self.0)
    }

    /// Casts to any integer type returning `None` on overflow.
    pub const fn checked_cast<N>(self) -> Option<N>
    where
        N: [const] CheckedCastFromInt<usize>,
    {
        N::checked_cast_from(self.0)
    }

    /// Casts to any integer type saturating on overflow.
    pub const fn saturating_cast<N>(self) -> N
    where
        N: [const] BoundedCastFromInt<usize>,
    {
        N::saturating_cast_from(self.0)
    }

    /// Casts to any integer type panicking on overflow.
    pub const fn strict_cast<N>(self) -> N
    where
        N: [const] CheckedCastFromInt<usize>,
    {
        N::strict_cast_from(self.0)
    }

    /// Validates `self` is within upper bounds
    pub const fn is_valid(&self) -> bool {
        self <= &Self::MAX
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

    /// Replaces invalid values with [`Self::MAX`]. No need to saturate at `0` as this is covered
    /// by the internal representation.
    pub const fn saturate(self) -> Self {
        match self.is_valid() {
            true => self,
            false => Self::MAX,
        }
    }

    // ============================
    // Arithmetic: add / sub
    //
    // rhs is restricted by From
    // ============================

    /// Strict addition. Computes `self + rhs`, panicking if the result is above [`Self::MAX`].
    ///
    /// `rhs` may only be a `ByteSize`.
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented. (See [`Self::MAX`])
    pub const fn strict_add<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).0;
        let v = self.0.strict_add(rhs);
        Self::strict_new::<usize>(v)
    }

    /// Checked addition. Computes `self + rhs`, returning `None` if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may only be a `ByteSize`.
    pub const fn checked_add<R>(self, rhs: R) -> Option<Self>
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).0;
        let v = self.0.checked_add(rhs)?;
        Self::new::<usize>(v)
    }

    /// Saturating addition. Computes `self + rhs`, saturating at [`Self::MAX`].
    ///
    /// `rhs` may only be a `ByteSize`.
    pub const fn saturating_add<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).0;
        let v = self.0.saturating_add(rhs);
        Self::saturating_new::<usize>(v)
    }

    /// Strict subtraction. Computes `self - rhs`, panicking if `rhs` is greater than `self`.
    ///
    /// `rhs` may only be a `ByteSize`.
    ///
    /// # Panics
    /// This function will always panic on underflow, regardless of whether overflow checks are enabled.
    pub const fn strict_sub<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).0;
        let v = self.0.strict_sub(rhs);
        Self::strict_new::<usize>(v)
    }

    /// Checked subtraction. Computes `self - rhs`, returning `None` if `rhs` is greater than `self`.
    ///
    /// `rhs` may only be a `ByteSize`.
    pub const fn checked_sub<R>(self, rhs: R) -> Option<Self>
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).0;
        let v = self.0.checked_sub(rhs)?;
        Self::new::<usize>(v)
    }

    /// Saturating subtraction. Computes `self - rhs`, saturating at zero.
    ///
    /// `rhs` may only be a `ByteSize`.
    pub const fn saturating_sub<R>(self, rhs: R) -> Self
    where
        Self: [const] From<R>,
    {
        let rhs = Self::from(rhs).0;
        let v = self.0.saturating_sub(rhs);
        Self::saturating_new::<usize>(v)
    }

    // ============================
    // Arithmetic: mul
    //
    // rhs may only be a scalar N
    // ============================

    /// Strict multiplication. Computes `self * rhs`, panicking if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may be any scalar integer.
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented. (See [`Self::MAX`])
    pub const fn strict_mul<N>(self, rhs: N) -> Self
    where
        usize: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in a ByteSize and self is non-zero, then the product is too large
        let rhs = match self {
            Self::ZERO => Self::ZERO,
            _ => Self::strict_new(rhs),
        };
        let v = self.0.strict_mul(rhs.0);
        Self::strict_new::<usize>(v)
    }

    /// Checked multiplication. Computes `self * rhs`, returning `None` if the result is above
    /// [`Self::MAX`].
    ///
    /// `rhs` may be any scalar integer.
    pub const fn checked_mul<N>(self, rhs: N) -> Option<Self>
    where
        usize: [const] CheckedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in a ByteSize and self is non-zero, then the product is too large
        let rhs = match self {
            Self::ZERO => Self::ZERO,
            _ => Self::new(rhs)?,
        };
        let v = self.0.checked_mul(rhs.0)?;
        Self::new::<usize>(v)
    }

    /// Saturating multiplication. Computes `self * rhs`, saturating at [`Self::MAX`].
    ///
    /// `rhs` may be any scalar `N` integer
    pub const fn saturating_mul<N>(self, rhs: N) -> Self
    where
        usize: [const] BoundedCastFromInt<N>,
        N: [const] Destruct,
    {
        // If rhs won't fit in a ByteSize and self is non-zero, then the product is too large
        let rhs = match self {
            Self::ZERO => Self::ZERO,
            _ => Self::saturating_new(rhs),
        };
        let v = self.0.saturating_mul(rhs.0);
        Self::saturating_new::<usize>(v)
    }

    // ============================
    // Arithmetic: div / rem
    // ============================

    /// Basic division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_div & strict_div, as self is valid and the result of
    ///   division must always be 0..=self
    ///
    /// # Panics
    /// Panics if `rhs` is zero. The cast clamps `rhs` into `usize` range first: a negative `rhs`
    /// becomes 0 (and so also panics), while an `rhs` above `usize::MAX` becomes `usize::MAX` —
    /// larger than any valid `ByteSize`, so the result is [`ByteSize::ZERO`].
    pub const fn div<N>(self, rhs: N) -> Self
    where
        usize: [const] BoundedCastFromInt<N>,
    {
        let rhs = usize::saturating_cast_from(rhs);
        let v = self.0.div(rhs);
        Self::saturating_new::<usize>(v)
    }

    /// Checked division. Computes `self / rhs`, returning `None` if `rhs` clamps to zero
    /// (i.e. `rhs` is zero or negative).
    ///
    /// `rhs` may be any scalar integer. An `rhs` above `usize::MAX` clamps to `usize::MAX`, which is
    /// larger than any valid `ByteSize`, yielding `Some(ByteSize::ZERO)`.
    ///
    /// # Note
    /// - This is the only specialised division. Use `div` if you can accept a panic on div by zero.
    pub const fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        usize: [const] BoundedCastFromInt<N>,
    {
        let rhs = usize::saturating_cast_from(rhs);
        let v = self.0.checked_div(rhs)?;
        Self::new::<usize>(v)
    }

    /// Basic remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// # Note
    /// - This replaces both saturating_rem & strict_rem, as self is valid and the remainder from
    ///   division must always be 0..=self
    ///
    /// # Panics
    /// Panics if `rhs` is zero. The cast clamps `rhs` into `usize` range first: a negative `rhs`
    /// becomes 0 (and so also panics), while an `rhs` above `usize::MAX` becomes `usize::MAX` —
    /// larger than any valid `ByteSize`, so the result is `self`.
    pub const fn rem<N>(self, rhs: N) -> Self
    where
        usize: [const] BoundedCastFromInt<N>,
    {
        let rhs = usize::saturating_cast_from(rhs);
        let v = self.0.rem(rhs);
        Self::saturating_new::<usize>(v)
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` if `rhs` clamps to zero
    /// (i.e. `rhs` is zero or negative).
    ///
    /// `rhs` may be any scalar integer. An `rhs` above `usize::MAX` clamps to `usize::MAX`, which is
    /// larger than any valid `ByteSize`, yielding `Some(self)`.
    ///
    /// # Note
    /// - This is the only specialised remainder. Use `rem` if you can accept a panic on div by zero.
    pub const fn checked_rem<N>(self, rhs: N) -> Option<Self>
    where
        usize: [const] BoundedCastFromInt<N>,
    {
        let rhs = usize::saturating_cast_from(rhs);
        let v = self.0.checked_rem(rhs)?;
        Self::new::<usize>(v)
    }

    // ============================
    // Conversion to Frame counts
    //
    // divides by Frame::SIZE_IN_BYTES
    // ============================

    /// The number of [`Frame`]s contained in this byte size, panicking unless `self` is an
    /// exact multiple of [`Frame::SIZE_IN_BYTES`].
    ///
    /// # Panics
    /// Panics if `self` is not an exact multiple of [`Frame::SIZE_IN_BYTES`].
    pub const fn frames_exact(self) -> Frame {
        match self.frames_exact_checked() {
            Some(frames) => frames,
            None => panic!("byte size is not an exact number of frames"),
        }
    }

    /// The number of [`Frame`]s contained in this byte size, returning `None` unless `self`
    /// is an exact multiple of [`Frame::SIZE_IN_BYTES`].
    ///
    /// # Note
    /// - thanks to the [`ByteSize`] invariant (`self <= `[`ByteSize::MAX`]`), the result is
    ///   always within [`Frame::MAX`], so `None` here only ever means "not an exact number of
    ///   frames"
    ///
    /// # Examples
    /// ```
    /// use redbook::{Frame, frame::ByteSize};
    ///
    /// assert_eq!(
    ///     ByteSize::strict_new(2 * 2352).frames_exact_checked(),
    ///     Some(Frame::strict_new(2))
    /// );
    /// assert_eq!(ByteSize::strict_new(2353).frames_exact_checked(), None);
    /// ```
    pub const fn frames_exact_checked(self) -> Option<Frame> {
        let frame_size = Frame::SIZE_IN_BYTES.cast::<usize>();
        match self.0 % frame_size {
            0 => Some(Frame::strict_new(self.0 / frame_size)),
            _ => None,
        }
    }

    /// The number of whole [`Frame`]s contained in this byte size:
    /// `self / `[`Frame::SIZE_IN_BYTES`], rounded down.
    ///
    /// # Note
    /// - thanks to the [`ByteSize`] invariant (`self <= `[`ByteSize::MAX`]`), the result is
    ///   always within [`Frame::MAX`], so there are deliberately no checked / saturating /
    ///   strict variants of this method
    pub const fn frames_floor(self) -> Frame {
        Frame::strict_new(self.0 / Frame::SIZE_IN_BYTES.cast::<usize>())
    }
}

// ===========================================
// ByteSize - const comparison & conversion ops
// ===========================================

const impl Ord for ByteSize {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

const impl PartialOrd for ByteSize {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const impl Eq for ByteSize {}

const impl PartialEq for ByteSize {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl StructuralPartialEq for ByteSize {}

impl Display for ByteSize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} bytes", self.0)
    }
}

impl FromStr for ByteSize {
    type Err = ParseIntError;

    /// Parses a size in bytes, failing if the value is above [`ByteSize::MAX`].
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let inner: usize = s.parse()?;
        match Self::new(inner) {
            Some(size) => Ok(size),
            // `usize::MAX + 1` always fails to parse as a `usize`, yielding a PosOverflow
            // error, which is also the right error for a semantic overflow above MAX
            None => usize::MAX
                .strict_cast::<u128>()
                .strict_add(1)
                .to_string()
                .parse::<usize>()
                .map(|_| Self::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{MAX_CHUNK_BYTES, MAX_CHUNK_FRAMES};

    use rstest::rstest;

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
    fn as_u8s_returns_components() {
        assert_eq!(Msf::new(0, 2, 0).as_u8s(), (0, 2, 0));
        assert_eq!(Msf::new(0xFF, 0xFF, 0xFF).as_u8s(), (0xFF, 0xFF, 0xFF));
        // degenerate components are returned as-is: Msf performs no normalisation
        assert_eq!(Msf::new(1, 60, 75).as_u8s(), (1, 60, 75));
    }

    #[test]
    fn pattern_match_on_constants_compares_by_value() {
        // constant arms match via StructuralPartialEq, i.e. by value not by identity
        let frame = Frame::strict_new(150);
        let description = match frame {
            Frame::ZERO => "zero",
            LEADIN => "lead-in",
            _ => "other",
        };
        assert_eq!(description, "lead-in");
    }

    #[test]
    fn leadin_compensation() {
        let starting_frames = Frame::strict_new(183);
        let starting_time = Msf::new(0, 2, 33);
        assert_eq!(starting_frames.relative_to_leadin(), Frame::strict_new(33));
        assert_eq!(starting_time.relative_to_leadin(), Msf::new(0, 0, 33));
    }

    #[rstest]
    #[case((0,0,0))]
    #[case((0,2,0))]
    #[case((5,0,2))]
    #[case((15,0,74))]
    #[case((15,59,0))]
    #[case((20,3,74))]
    #[case((20,59,3))]
    #[case((25,59,74))]
    #[case((60,0,74))]
    #[case((60,5,74))]
    #[case((60,59,0))]
    #[case((60,59,5))]
    #[case((60,59,74))]
    #[case((0xFF,59,74))]
    #[case((0xFF,0xFF,0xFF))]
    fn interconversion(#[case] msf: (u8, u8, u8)) {
        let msf1 = Msf::new.call(msf);
        let frames = Frame::from(msf1);
        let msf2 = Msf::from(frames);
        assert_eq!(msf1, msf2);
    }

    #[rstest]
    #[case((0,0,0xFF), (0,3,30))]
    #[case((0,0xFF,0), (4,15,0))]
    #[case((1,0,0xFF), (1,3,30))]
    #[case((1,0xFF,0), (5,15,0))]
    #[case((2,1,0xFF), (2,4,30))]
    #[case((2,0xFF,1), (6,15,1))]
    fn degenerate_interconversion(#[case] msf: (u8, u8, u8), #[case] expected: (u8, u8, u8)) {
        let msf1 = Msf::new.call(msf);
        let frames = Frame::from(msf1);
        let msf2 = Msf::from(frames);
        assert_eq!(msf2, Msf::new.call(expected));
    }

    #[rstest]
    #[case(2, u32::MAX, None)]
    #[case(2, u64::from(u32::MAX) + 1, None)]
    #[case(0, u32::MAX, Some(Frame::ZERO))]
    #[case(3, 5_u32, Some(Frame::strict_new(15)))]
    fn checked_mul_out_of_range<L, R>(
        #[case] lhs: L,
        #[case] rhs: R,
        #[case] expected: Option<Frame>,
    ) where
        u32: CheckedCastFromInt<L> + CheckedCastFromInt<R>,
    {
        assert_eq!(Frame::strict_new(lhs).checked_mul(rhs), expected);
    }

    #[rstest]
    #[case(1_u8)]
    #[case(Frame::MAX.cast::<u32>())]
    #[case(u64::from(u32::MAX) + 1)]
    #[case(i128::MAX)]
    fn checked_div<R>(#[case] rhs: R)
    where
        u32: BoundedCastFromInt<R>,
        R: Copy,
    {
        let frame = Frame::strict_new(2000);
        let checked_div = frame.checked_div(rhs).unwrap();
        assert_eq!(
            checked_div.cast::<u32>(),
            2000u32 / u32::saturating_cast_from(rhs)
        );
        assert_eq!(frame.div(rhs), checked_div);
    }

    #[rstest]
    #[case(i128::MIN)]
    #[case(-1_i8)]
    #[case(0_u32)]
    #[should_panic(expected = "attempt to divide by zero")]
    fn checked_div_by_zero<R>(#[case] rhs: R)
    where
        u32: BoundedCastFromInt<R>,
        R: Copy,
    {
        let frame = Frame::strict_new(2000);
        assert!(frame.checked_div(rhs).is_none());
        frame.div(rhs);
    }

    #[rstest]
    #[case(1_u8)]
    #[case(Frame::MAX.cast::<u32>())]
    #[case(u64::from(u32::MAX) + 1)]
    #[case(i128::MAX)]
    fn checked_rem<R>(#[case] rhs: R)
    where
        u32: BoundedCastFromInt<R>,
        R: Copy,
    {
        let frame = Frame::strict_new(2000);
        let checked_rem = frame.checked_rem(rhs).unwrap();
        assert_eq!(
            checked_rem.cast::<u32>(),
            2000u32 % u32::saturating_cast_from(rhs)
        );
        assert_eq!(frame.rem(rhs), checked_rem);
    }

    #[rstest]
    #[case(i128::MIN)]
    #[case(-1_i8)]
    #[case(0_u32)]
    #[should_panic(expected = "attempt to calculate the remainder with a divisor of zero")]
    fn checked_rem_div_zero<R>(#[case] rhs: R)
    where
        u32: BoundedCastFromInt<R>,
        R: Copy,
    {
        let frame = Frame::strict_new(2000);
        assert!(frame.checked_rem(rhs).is_none());
        frame.rem(rhs);
    }

    #[test]
    fn bytesize_usable_in_const_context() {
        // compile-time proof: these only compile if ByteSize & Frame::size are usable in const contexts
        const SIZE_IN_BYTES: ByteSize = Frame::SIZE_IN_BYTES;
        const ONE_FRAME: ByteSize = Frame::strict_new(1).size();
        const MAX: ByteSize = ByteSize::MAX;
        const FRAMES_IN_64KB: Frame = ByteSize(64 * 1024).frames_floor();
        const EXACT: Option<Frame> = ByteSize(2 * 2352).frames_exact_checked();
        const NOT_EXACT: Option<Frame> = ByteSize(2353).frames_exact_checked();
        const CAST: usize = SIZE_IN_BYTES.cast();
        const CAST_U64: u64 = SIZE_IN_BYTES.strict_cast();
        const AS_USIZE: usize = ByteSize(65536).strict_cast();
        const EQ: bool = SIZE_IN_BYTES == ByteSize(2352);
        const NE: bool = SIZE_IN_BYTES != ByteSize(2353);
        const LT: bool = ByteSize(1) < ByteSize(2);

        // cross-check the const-evaluated values against runtime evaluation
        assert_eq!(SIZE_IN_BYTES, ByteSize(2352));
        assert_eq!(ONE_FRAME, ByteSize(2352));
        assert_eq!(MAX, Frame::MAX.size());
        assert_eq!(FRAMES_IN_64KB, Frame::strict_new(27));
        assert_eq!(EXACT, Some(Frame::strict_new(2)));
        assert_eq!(NOT_EXACT, None);
        assert_eq!(CAST, 2352);
        assert_eq!(CAST_U64, 2352_u64);
        assert_eq!(AS_USIZE, 65536);
        let size_in_bytes = Frame::SIZE_IN_BYTES;
        assert_eq!(EQ, size_in_bytes == ByteSize(2352));
        assert_eq!(NE, size_in_bytes != ByteSize(2353));
        assert_eq!(LT, ByteSize(1) < size_in_bytes);
    }

    #[test]
    fn bytesize_max_is_frame_max_size() {
        // ByteSize::MAX is defined as Frame::MAX.size(), so a ByteSize always interconvertible
        // with a Frame count
        assert_eq!(ByteSize::MAX, Frame::MAX.size());
        assert_eq!(ByteSize::MAX.strict_cast::<u64>(), 2_744_501_760); // 0xA395_C600
        assert!(ByteSize::MAX.is_valid());
        assert_eq!(ByteSize::MAX.validate(), Some(ByteSize::MAX));
        assert_eq!(ByteSize::MAX.frames_floor(), Frame::MAX);
        assert_eq!(ByteSize::MAX.frames_exact(), Frame::MAX);
        // arithmetic saturates at MAX
        assert_eq!(ByteSize::MAX.saturating_add(ByteSize(1)), ByteSize::MAX);
        assert_eq!(ByteSize::MAX.saturating_mul(2), ByteSize::MAX);
        assert!(ByteSize::MAX.checked_add(ByteSize(1)).is_none());
    }

    #[test]
    fn frame_size_round_trips_through_bytesize() {
        for frames in [0, 1, 75, 150, 2000, Frame::MAX.cast::<u32>()] {
            let frame = Frame::strict_new(frames);
            let size = frame.size();
            assert_eq!(size, ByteSize::strict_new(frames * 2352));
            assert_eq!(size.frames_floor(), frame);
            assert_eq!(size.frames_exact(), frame);
            assert_eq!(size.frames_exact_checked(), Some(frame));
        }
        // Frame::MAX * SIZE_IN_BYTES is exactly ByteSize::MAX
        assert_eq!(Frame::MAX.size(), ByteSize::MAX);
    }

    #[rstest]
    #[case(2353, Frame::strict_new(1), None)]
    #[case(2_744_501_759, Frame::strict_new(1_166_879), None)] // ByteSize::MAX - 1
    #[case(2_744_501_760, Frame::MAX, Some(Frame::MAX))] // ByteSize::MAX
    fn bytesize_frames_conversions(
        #[case] bytes: usize,
        #[case] expected_floor: Frame,
        #[case] expected_exact: Option<Frame>,
    ) {
        let size = ByteSize::strict_new(bytes);
        // floor division rounds down; thanks to the ByteSize invariant it is always within
        // Frame::MAX, so no checked/saturating/strict variants exist
        assert_eq!(size.frames_floor(), expected_floor);
        // exact division additionally requires a whole number of frames
        assert_eq!(size.frames_exact_checked(), expected_exact);
    }

    #[test]
    fn bytesize_new_returns_none_above_max() {
        // ByteSize::MAX + 1 and anything larger is rejected...
        assert_eq!(ByteSize::new(2_744_501_761_u64), None);
        assert_eq!(ByteSize::new(u128::MAX), None);
        // ...but saturates with saturating_new
        assert_eq!(ByteSize::saturating_new(2_744_501_761_u64), ByteSize::MAX);
        assert_eq!(ByteSize::saturating_new(u128::MAX), ByteSize::MAX);
    }

    #[test]
    #[should_panic(expected = "semantic overflow")]
    fn bytesize_strict_new_panics_above_max() {
        ByteSize::strict_new(2_744_501_761_u64);
    }

    #[test]
    #[should_panic(expected = "not an exact number of frames")]
    fn bytesize_exact_frames_panics_when_not_exact() {
        ByteSize(2353).frames_exact();
    }

    #[rstest]
    #[case(42_usize)]
    #[case(2352)]
    #[case(ByteSize::MAX.cast())]
    fn bytesize_constructors_casts_and_raw_value_access(#[case] raw: usize) {
        let size = ByteSize::strict_new(raw);

        // constructors from any integer type which fits & is within MAX
        assert_eq!(ByteSize::new(raw), Some(size));
        assert_eq!(ByteSize::strict_new(raw), size);
        assert_eq!(ByteSize::saturating_new(raw), size);
        // integers which do not fit in a usize, or which are above ByteSize::MAX, are rejected
        assert_eq!(ByteSize::new(42_u8), Some(ByteSize(42)));
        assert_eq!(ByteSize::new(-1_i32), None);
        assert_eq!(ByteSize::new(2_744_501_761_u64), None);
        assert_eq!(ByteSize::new(u128::MAX), None);
        assert_eq!(ByteSize::saturating_new(-1_i32), ByteSize::ZERO);
        assert_eq!(ByteSize::saturating_new(u128::MAX), ByteSize::MAX);

        // retrieval of the inner value via casts
        assert_eq!(size.cast::<usize>(), raw);
        assert_eq!(size.strict_cast::<u64>(), u64::try_from(raw).unwrap());
        assert_eq!(size.strict_cast::<u128>(), u128::try_from(raw).unwrap());
        assert_eq!(size.checked_cast::<u32>(), u32::try_from(raw).ok());
        assert_eq!(size.checked_cast::<u16>(), u16::try_from(raw).ok());
        assert_eq!(size.saturating_cast::<u32>(), u32::try_from(raw).unwrap());
        assert_eq!(ByteSize(42).strict_cast::<u16>(), 42);

        // parsing & display
        assert_eq!(raw.to_string().parse::<ByteSize>(), Ok(size));
        assert!("2744501761".parse::<ByteSize>().is_err()); // ByteSize::MAX + 1
        assert!("-1".parse::<ByteSize>().is_err());
        assert_eq!(size.to_string(), format!("{raw} bytes"));

        // arithmetic is via the strict_/checked_/saturating_ methods (no Deref, no operators)
        assert_eq!(ByteSize(1).strict_add(ByteSize(1)), ByteSize(2));
        assert_eq!(ByteSize(2).checked_mul(3), Some(ByteSize(6)));
        assert_eq!(ByteSize(7).saturating_sub(ByteSize(10)), ByteSize::ZERO);
    }

    #[test]
    fn max_chunk_sizes_are_consistent() {
        assert!(MAX_CHUNK_BYTES < ByteSize(64 * 1024));
        assert_eq!(MAX_CHUNK_FRAMES.size(), MAX_CHUNK_BYTES);
        assert_eq!(MAX_CHUNK_BYTES.frames_exact(), MAX_CHUNK_FRAMES);
    }
}
