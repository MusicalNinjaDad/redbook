//! Time units: Audio CDs measure time in 1/75 second "frames". Provides `Frame` & `Msf`
//! (min, sec, frame).

use std::{fmt::Display, num::TryFromIntError};

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

impl Frame {
    /// Given [constant improvements in manufacturing][wiki], this represent the maximum Frame value
    /// which can be represented in a TOC or other sub-channel data which stores times as
    /// MSF(u8,u8,u8). It is certainly slightly oversized, but some crazy person may choose to ignore
    /// that there are only 60 seconds in a minute and store an MSF of (FF, FF, FF) for some reason.
    ///
    /// [wiki]: https://en.wikipedia.org/wiki/Compact_Disc_Digital_Audio#Storage_capacity_and_playing_time
    pub const MAX: Self = Frame((0xFF * 60 * 75) + (0xFF * 75) + 0xFF);

    /// The number of representable frame values (`MAX + 1`).
    ///
    /// Used by the `wrapping_*` methods, which wrap around within the representable range
    /// (`0..=MAX`) rather than the full `u32` range, so that the [`Frame`] invariant of always
    /// being convertible to an [`Msf`] is upheld.
    const RANGE: u32 = Self::MAX.0 + 1;

    /// Creates a new `Frame` from any integer type, returning `None` if the value does not fit
    /// in a `u32` or is above [`Frame::MAX`].
    ///
    /// This is the validated, non-panicking constructor: it is not blind. Accepting any integer
    /// type also helps to avoid excessive casting when creating a `Frame` from e.g. a `usize`.
    /// For a panicking `const` constructor use [`Frame::strict_new`].
    ///
    /// # Returns
    /// - `Some(frame)` if `frames` fits in a `u32` and is `<=` [`Frame::MAX`]
    /// - `None` if `frames` does not fit in a `u32`, or if the resulting value is above the
    ///   maximum which can be semantically represented as a `Frame`. (See [`Frame::MAX`])
    pub fn new<N>(frames: N) -> Option<Self>
    where
        u32: TryFrom<N>,
    {
        let frames = u32::try_from(frames).ok()?;
        if frames <= Self::MAX.0 {
            Some(Self(frames))
        } else {
            None
        }
    }

    /// Creates a new `Frame` from a `u32`, panicking if the value is above [`Frame::MAX`].
    ///
    /// This is the const constructor of choice: when used in a const context the validation
    /// happens at compile time.
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented as a `Frame`. (See [`Frame::MAX`])
    pub const fn strict_new(frames: u32) -> Self {
        assert!(
            frames <= Self::MAX.0,
            "requested frames cannot be represented for an audio CD"
        );
        Self(frames)
    }

    /// Cast to another integer type (where `N: From<u32>`).
    pub const fn cast<N>(self) -> N
    where
        N: [const] From<u32>,
    {
        N::from(self.0)
    }

    /// Returns a frame relative to the lead-in position.
    ///
    /// This subtracts the standard 150-frame lead-in from the frame position,
    /// giving the position relative to the start of the actual audio data.
    ///
    /// # Note
    ///
    /// To guard against miguided attempts to "copy protect" CDs by breaking them, this will
    /// never return negative. So a badly constructed TOC with track 1 starting at frame 147
    /// (0m:1s:72f) will return frame 0 relative to leadin.
    pub const fn relative_to_leadin(self) -> Self {
        self.saturating_sub(LEADIN)
    }

    // ============================
    // Arithmetic: add / sub / rem
    //
    // rhs may be a Frame or an Msf (anything which converts into a Frame — and the only such
    // conversions are between Frame & Msf).
    // ============================

    /// Strict addition. Computes `self + rhs`, panicking if the result is above [`Frame::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented as a `Frame`. (See [`Frame::MAX`])
    pub const fn strict_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let rhs = Frame::from(rhs).0;
        let f = Self(self.0.strict_add(rhs));
        assert!(
            f.0 <= Self::MAX.0,
            "requested frames cannot be represented for an audio CD"
        );
        f
    }

    /// Checked addition. Computes `self + rhs`, returning `None` if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_add<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        let rhs = Frame::from(rhs).0;
        match self.0.checked_add(rhs) {
            Some(f) if f <= Self::MAX.0 => Some(Self(f)),
            _ => None,
        }
    }

    /// Saturating addition. Computes `self + rhs`, saturating at [`Frame::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let rhs = Frame::from(rhs).0;
        let f = self.0.saturating_add(rhs);
        if f > Self::MAX.0 { Self::MAX } else { Self(f) }
    }

    /// Wrapping addition. Computes `self + rhs`, wrapping around within the representable range
    /// (`0..=MAX`, i.e. modulo `MAX + 1`) so the result can always be converted to an [`Msf`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn wrapping_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let rhs = Frame::from(rhs).0;
        Self(((self.0 as u64 + rhs as u64) % Self::RANGE as u64) as u32)
    }

    /// Strict subtraction. Computes `self - rhs`, panicking if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// This function will always panic on underflow, regardless of whether overflow checks are enabled.
    pub const fn strict_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self(self.0.strict_sub(Frame::from(rhs).0))
    }

    /// Checked subtraction. Computes `self - rhs`, returning `None` if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_sub<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        match self.0.checked_sub(Frame::from(rhs).0) {
            Some(f) => Some(Self(f)),
            None => None,
        }
    }

    /// Saturating subtraction. Computes `self - rhs`, saturating at zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self(self.0.saturating_sub(Frame::from(rhs).0))
    }

    /// Wrapping subtraction. Computes `self - rhs`, wrapping around within the representable
    /// range (`0..=MAX`, i.e. modulo `MAX + 1`) so the result can always be converted to an [`Msf`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn wrapping_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        let rhs = Frame::from(rhs).0;
        Self(((self.0 as u64 + Self::RANGE as u64 - rhs as u64) % Self::RANGE as u64) as u32)
    }

    /// Strict remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// This function will always panic on division by zero, regardless of whether overflow checks
    /// are enabled.
    pub const fn strict_rem<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self(self.0.strict_rem(Frame::from(rhs).0))
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_rem<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        match self.0.checked_rem(Frame::from(rhs).0) {
            Some(f) => Some(Self(f)),
            None => None,
        }
    }

    /// Wrapping remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// This function will always panic on division by zero, regardless of whether overflow checks
    /// are enabled.
    pub const fn wrapping_rem<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self(self.0.wrapping_rem(Frame::from(rhs).0))
    }

    // ============================
    // Arithmetic: mul / div
    //
    // rhs may only be a scalar N where u32: From<N> (e.g. u8, u16, u32) — deliberately not a
    // Frame or Msf. Use .cast() first if you have a Frame/Msf scalar.
    // ============================

    /// Strict multiplication. Computes `self * rhs`, panicking if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    ///
    /// # Panics
    /// ## Overflow behavior
    /// This function will always panic on overflow, regardless of whether overflow checks are enabled.
    ///
    /// ## Semantic behaviour
    /// This function will panic if the resulting value is above the maximum which can be
    /// semantically represented as a `Frame`. (See [`Frame::MAX`])
    pub const fn strict_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] From<N>,
    {
        let f = Self(self.0.strict_mul(u32::from(rhs)));
        assert!(
            f.0 <= Self::MAX.0,
            "requested frames cannot be represented for an audio CD"
        );
        f
    }

    /// Checked multiplication. Computes `self * rhs`, returning `None` if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    pub const fn checked_mul<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] From<N>,
    {
        match self.0.checked_mul(u32::from(rhs)) {
            Some(f) if f <= Self::MAX.0 => Some(Self(f)),
            _ => None,
        }
    }

    /// Saturating multiplication. Computes `self * rhs`, saturating at [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    pub const fn saturating_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] From<N>,
    {
        let f = self.0.saturating_mul(u32::from(rhs));
        if f > Self::MAX.0 { Self::MAX } else { Self(f) }
    }

    /// Wrapping multiplication. Computes `self * rhs`, wrapping around within the representable
    /// range (`0..=MAX`, i.e. modulo `MAX + 1`) so the result can always be converted to an [`Msf`].
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    pub const fn wrapping_mul<N>(self, rhs: N) -> Self
    where
        u32: [const] From<N>,
    {
        Self(((self.0 as u64 * u32::from(rhs) as u64) % Self::RANGE as u64) as u32)
    }

    /// Strict division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    ///
    /// # Panics
    /// This function will always panic on division by zero, regardless of whether overflow checks
    /// are enabled.
    pub const fn strict_div<N>(self, rhs: N) -> Self
    where
        u32: [const] From<N>,
    {
        Self(self.0.strict_div(u32::from(rhs)))
    }

    /// Checked division. Computes `self / rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    pub const fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        u32: [const] From<N>,
    {
        match self.0.checked_div(u32::from(rhs)) {
            Some(f) => Some(Self(f)),
            None => None,
        }
    }

    /// Saturating division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    ///
    /// # Panics
    /// This function will always panic on division by zero, regardless of whether overflow checks
    /// are enabled.
    pub const fn saturating_div<N>(self, rhs: N) -> Self
    where
        u32: [const] From<N>,
    {
        Self(self.0.saturating_div(u32::from(rhs)))
    }

    /// Wrapping division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u32: From<N>` (e.g. `u8`, `u16`, `u32`).
    ///
    /// # Panics
    /// This function will always panic on division by zero, regardless of whether overflow checks
    /// are enabled.
    pub const fn wrapping_div<N>(self, rhs: N) -> Self
    where
        u32: [const] From<N>,
    {
        Self(self.0.wrapping_div(u32::from(rhs)))
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
    // rhs may be a Frame or an Msf. Computed via Frame so results are normalised and the
    // Frame::MAX bound is respected.
    // ============================

    /// Strict addition. Computes `self + rhs`, panicking if the result is above [`Frame::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::strict_add`].
    pub const fn strict_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).strict_add(rhs))
    }

    /// Checked addition. Computes `self + rhs`, returning `None` if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_add<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        match <Frame as From<Msf>>::from(self).checked_add(rhs) {
            Some(f) => Some(Self::from(f)),
            None => None,
        }
    }

    /// Saturating addition. Computes `self + rhs`, saturating at [`Frame::MAX`].
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).saturating_add(rhs))
    }

    /// Wrapping addition. Computes `self + rhs`, wrapping around within the representable range.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn wrapping_add<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).wrapping_add(rhs))
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
        Self::from(<Frame as From<Msf>>::from(self).strict_sub(rhs))
    }

    /// Checked subtraction. Computes `self - rhs`, returning `None` if `rhs` is greater than `self`.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_sub<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        match <Frame as From<Msf>>::from(self).checked_sub(rhs) {
            Some(f) => Some(Self::from(f)),
            None => None,
        }
    }

    /// Saturating subtraction. Computes `self - rhs`, saturating at zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn saturating_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).saturating_sub(rhs))
    }

    /// Wrapping subtraction. Computes `self - rhs`, wrapping around within the representable range.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn wrapping_sub<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).wrapping_sub(rhs))
    }

    /// Strict remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::strict_rem`].
    pub const fn strict_rem<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).strict_rem(rhs))
    }

    /// Checked remainder. Computes `self % rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    pub const fn checked_rem<R>(self, rhs: R) -> Option<Self>
    where
        Frame: [const] From<R>,
    {
        match <Frame as From<Msf>>::from(self).checked_rem(rhs) {
            Some(f) => Some(Self::from(f)),
            None => None,
        }
    }

    /// Wrapping remainder. Computes `self % rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be a [`Frame`] or an [`Msf`].
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::wrapping_rem`].
    pub const fn wrapping_rem<R>(self, rhs: R) -> Self
    where
        Frame: [const] From<R>,
    {
        Self::from(<Frame as From<Msf>>::from(self).wrapping_rem(rhs))
    }

    // ============================
    // Arithmetic: mul / div
    //
    // rhs may only be a scalar N where u8: From<N> (i.e. a u8) — deliberately not a Frame or Msf.
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
        u8: [const] From<N>,
    {
        Self::from(<Frame as From<Msf>>::from(self).strict_mul(u32::from(u8::from(rhs))))
    }

    /// Checked multiplication. Computes `self * rhs`, returning `None` if the result is above
    /// [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn checked_mul<N>(self, rhs: N) -> Option<Self>
    where
        u8: [const] From<N>,
    {
        match <Frame as From<Msf>>::from(self).checked_mul(u32::from(u8::from(rhs))) {
            Some(f) => Some(Self::from(f)),
            None => None,
        }
    }

    /// Saturating multiplication. Computes `self * rhs`, saturating at [`Frame::MAX`].
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn saturating_mul<N>(self, rhs: N) -> Self
    where
        u8: [const] From<N>,
    {
        Self::from(<Frame as From<Msf>>::from(self).saturating_mul(u32::from(u8::from(rhs))))
    }

    /// Wrapping multiplication. Computes `self * rhs`, wrapping around within the representable range.
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn wrapping_mul<N>(self, rhs: N) -> Self
    where
        u8: [const] From<N>,
    {
        Self::from(<Frame as From<Msf>>::from(self).wrapping_mul(u32::from(u8::from(rhs))))
    }

    /// Strict division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::strict_div`].
    pub const fn strict_div<N>(self, rhs: N) -> Self
    where
        u8: [const] From<N>,
    {
        Self::from(<Frame as From<Msf>>::from(self).strict_div(u32::from(u8::from(rhs))))
    }

    /// Checked division. Computes `self / rhs`, returning `None` if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    pub const fn checked_div<N>(self, rhs: N) -> Option<Self>
    where
        u8: [const] From<N>,
    {
        match <Frame as From<Msf>>::from(self).checked_div(u32::from(u8::from(rhs))) {
            Some(f) => Some(Self::from(f)),
            None => None,
        }
    }

    /// Saturating division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::saturating_div`].
    pub const fn saturating_div<N>(self, rhs: N) -> Self
    where
        u8: [const] From<N>,
    {
        Self::from(<Frame as From<Msf>>::from(self).saturating_div(u32::from(u8::from(rhs))))
    }

    /// Wrapping division. Computes `self / rhs`, panicking if `rhs` is zero.
    ///
    /// `rhs` may be any scalar `N` where `u8: From<N>` (i.e. a `u8`).
    ///
    /// # Panics
    /// Panics under the same conditions as [`Frame::wrapping_div`].
    pub const fn wrapping_div<N>(self, rhs: N) -> Self
    where
        u8: [const] From<N>,
    {
        Self::from(<Frame as From<Msf>>::from(self).wrapping_div(u32::from(u8::from(rhs))))
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
    fn from(frames: Frame) -> Self {
        let frames = frames.cast::<u32>();
        let secs = frames / 75;
        let min = secs / 60;
        let secs = secs % 60;
        let frames = frames % 75;
        Self {
            min: min as u8,
            sec: secs as u8,
            frame: frames as u8,
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
        assert_eq!(Frame::MAX.wrapping_add(f(1)), f(0));

        // sub
        assert_eq!(f(5).strict_sub(f(3)), f(2));
        assert_eq!(f(5).strict_sub(Msf::new(0, 0, 3)), f(2));
        assert_eq!(f(1).checked_sub(f(2)), None);
        assert_eq!(f(1).saturating_sub(f(2)), f(0));
        assert_eq!(f(0).wrapping_sub(f(1)), Frame::MAX);

        // rem
        assert_eq!(f(150).strict_rem(Msf::new(0, 0, 7)), f(3));
        assert_eq!(f(150).checked_rem(f(0)), None);
        assert_eq!(f(150).wrapping_rem(f(7)), f(3));
    }

    #[test]
    fn frame_mul_div_flavours_take_scalars_only() {
        assert_eq!(f(2).strict_mul(75_u8), f(150));
        assert_eq!(f(2).strict_mul(75_u16), f(150));
        assert_eq!(f(150).strict_div(75_u32), f(2));
        assert_eq!(Frame::MAX.checked_mul(2_u32), None);
        assert_eq!(Frame::MAX.saturating_mul(2_u32), Frame::MAX);
        assert_eq!(f(7).wrapping_mul(3_u32), f(21));
        assert_eq!(f(150).checked_div(0_u32), None);
        assert_eq!(f(150).saturating_div(75_u8), f(2));
        assert_eq!(f(150).wrapping_div(75_u8), f(2));
    }

    #[test]
    #[should_panic(expected = "cannot be represented for an audio CD")]
    fn strict_add_panics_above_max() {
        let _: Frame = Frame::MAX.strict_add(f(1));
    }

    #[test]
    #[should_panic(expected = "attempt to subtract with overflow")]
    fn strict_sub_panics_on_underflow() {
        let _: Frame = f(1).strict_sub(f(2));
    }

    #[test]
    #[should_panic(expected = "attempt to divide by zero")]
    fn strict_div_panics_on_zero() {
        let _: Frame = f(1).strict_div(0_u32);
    }

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
        assert_eq!(Msf::new(2, 0, 0).strict_div(2_u8), Msf::new(1, 0, 0));
        assert_eq!(
            Msf::new(0, 2, 33).strict_rem(Msf::new(0, 0, 7)),
            Msf::new(0, 0, 1)
        );
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
}
