//! Time units: Audio CDs measure time in 1/75 second "frames". Provides `Frame` & `Msf`
//! (min, sec, frame).

use std::{
    cmp::max,
    fmt::Display,
    ops::{Add, AddAssign, Mul, Rem, Sub, SubAssign},
};

use crate::LEADIN;

/// CD audio frame (1/75 sec). Basic unit of time for CD audio.
///
/// A frame represents a single unit of CD audio data, which is 1/75th of a second.
/// This is the fundamental unit of time measurement for CD audio.
///
/// # TODOs
/// - TODO: #106 Repr transparent, store u-whatever, as_blah casts, etc for Frame, TrackNo, etc...
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct Frame(usize);

/// TODO: #124 currently outputs "Frame n" should probably just be "n", or 2 types: position & duration
impl Display for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame {}", self.0)
    }
}

impl Frame {
    /// Given [constant improvements in manufacturing][wiki], this represent the maximum Frame value
    /// which can be stored on the current architecture (`usize::MAX`) and *not* the current
    /// maximum number of frames which can be squeezed onto a single physical CD.
    ///
    /// TODO: #125 change Frame::MAX to be max referencable in TOC via MSF(u8,u8,u8)
    ///
    /// [wiki]: https://en.wikipedia.org/wiki/Compact_Disc_Digital_Audio#Storage_capacity_and_playing_time
    pub const MAX: Self = Frame(usize::MAX);

    /// Creates a new `Frame` from a frame count.
    pub const fn new(frames: usize) -> Self {
        Self(frames)
    }

    /// Returns the frame count as a `usize`.
    pub const fn as_usize(self) -> usize {
        self.0
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
        max(self - LEADIN, Frame(0))
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
    pub fn relative_to_leadin(self) -> Self {
        Self::from(Frame::from(self).relative_to_leadin())
    }
}

// ===============================================
// Frame & Msf - const interconversion & basic ops
// ===============================================

const impl From<Msf> for Frame {
    fn from(msf: Msf) -> Self {
        Self((((msf.min as usize * 60) + msf.sec as usize) * 75) + msf.frame as usize)
    }
}

const impl From<&Msf> for Frame {
    fn from(msf: &Msf) -> Self {
        Self::from(*msf)
    }
}

const impl From<Frame> for Msf {
    fn from(frames: Frame) -> Self {
        let frames = frames.as_usize();
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

const impl Add<Frame> for Frame {
    type Output = Self;

    fn add(self, rhs: Frame) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

const impl Add<Msf> for Frame {
    type Output = Frame;

    fn add(self, msf: Msf) -> Self::Output {
        self + Frame::from(msf)
    }
}

// --------------------------------
// Deliberately not implemented
//
// const impl Add<Frame> for Msf {}
//
// Frame should be primary unit of
// time (see note in Msf docs)
// --------------------------------

const impl Sub<Frame> for Frame {
    type Output = Self;

    fn sub(self, rhs: Frame) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

const impl Sub<Msf> for Frame {
    type Output = Frame;

    fn sub(self, msf: Msf) -> Self::Output {
        self - Frame::from(msf)
    }
}

// --------------------------------
// Deliberately not implemented
//
// const impl Sub<Frame> for Msf {}
//
// Frame should be primary unit of
// time (see note in Msf docs)
// --------------------------------

const impl Rem<Frame> for Frame {
    type Output = Self;

    fn rem(self, rhs: Self) -> Self::Output {
        Self(self.0 % rhs.0)
    }
}

const impl Rem<Msf> for Frame {
    type Output = Frame;

    fn rem(self, msf: Msf) -> Self::Output {
        self % Frame::from(msf)
    }
}

// --------------------------------
// Deliberately not implemented
//
// const impl Rem<Frame> for Msf {}
//
// Frame should be primary unit of
// time (see note in Msf docs)
// --------------------------------

// =======================
// Non-const blanket impls
// =======================

impl<N> Add<N> for Frame
where
    usize: Add<N, Output = usize>,
{
    type Output = Self;

    fn add(self, rhs: N) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl<N> AddAssign<N> for Frame
where
    Frame: Add<N, Output = Frame>,
{
    fn add_assign(&mut self, rhs: N) {
        let f = *self + rhs;
        self.0 = f.0
    }
}

impl<N> Sub<N> for Frame
where
    usize: Sub<N, Output = usize>,
{
    type Output = Self;

    fn sub(self, rhs: N) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl<N> SubAssign<N> for Frame
where
    Frame: Sub<N, Output = Frame>,
{
    fn sub_assign(&mut self, rhs: N) {
        let f = *self - rhs;
        self.0 = f.0
    }
}

impl<N> Rem<N> for Frame
where
    usize: Rem<N, Output = usize>,
{
    type Output = Self;

    fn rem(self, rhs: N) -> Self::Output {
        Self(self.0 % rhs)
    }
}

impl<N> Mul<N> for Frame
where
    usize: Mul<N, Output = usize>,
{
    type Output = Self;

    fn mul(self, rhs: N) -> Self::Output {
        Self(self.0 * rhs)
    }
}

// ===================================
// Inter-conversion with foreign types
//
// TODO #126 - drop conversions with time::Duration
// ===================================

impl PartialEq<cdtoc::Duration> for Frame {
    fn eq(&self, duration: &cdtoc::Duration) -> bool {
        let (s, f) = duration.seconds_frames();
        self.as_usize().strict_cast::<u64>() == s.strict_mul(75).strict_add(f.strict_cast())
    }
}

/// This is the format returned by cdtoc for [Track::msf()][cdtoc::Track::msf]
impl From<(u32, u8, u8)> for Msf {
    fn from(msf: (u32, u8, u8)) -> Self {
        let (m, sec, frame) = msf;
        Self {
            min: m.strict_cast(),
            sec,
            frame,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let starting_frames = Frame::new(183);
        let starting_time = Msf::new(0, 2, 33);
        assert_eq!(starting_frames.relative_to_leadin(), Frame::new(33));
        assert_eq!(starting_time.relative_to_leadin(), Msf::new(0, 0, 33));
    }
}
