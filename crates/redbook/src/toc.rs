//! Support for Tables of Contents & Frame-based time

use std::{
    collections::HashMap,
    fmt::Display,
    hint::cold_path,
    io::{self, ErrorKind},
    iter::Peekable,
    num::NonZeroUsize,
    ops::{Add, AddAssign, Mul, Rem, Sub, SubAssign},
    slice,
    time::Duration,
};

use bitflags::bitflags;
use cdtoc::Toc;
use itertools::{EitherOrBoth, Itertools};
use tracing_result::Trace;

use crate::{LEADIN, Track};

/// 0b0010 format track identifier for Leadout
pub const LEADOUT_TRACK_NUMBER: u8 = 0xA2;

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
/// # TODO
///
/// - Example
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TocEntry {
    /// The 1-indexed track number.
    pub track: u8,
    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    pub start: Frame,
    /// The control code, identifying track type
    pub control: Control,
}

impl Display for TocEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "track: {n}, start: {start}, control codes: {ctl}",
            n = self.track,
            start = self.start,
            ctl = self.control
        )
    }
}

impl PartialEq<cdtoc::Track> for TocEntry {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (self.track == toc_track.number()) && (self.start == Msf::from(toc_track.msf()))
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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

impl Control {
    /// represents a data track
    pub fn is_data(&self) -> bool {
        self.contains(Control::DATA_TRACK)
    }

    /// represents an audio track
    pub fn is_audio(&self) -> bool {
        !self.is_data()
    }
}

impl Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        bitflags::parser::to_writer_strict(self, f)
    }
}

impl TocEntry {
    /// Generate a TocEntry from a sequence of bytes as provided by MMC-3 command READ TOC
    /// format 0010b (Section 5.23.4)
    ///
    /// # Data format
    /// | byte  | contents |
    /// |------:|----------:|
    /// | 0     | Session Number |
    /// | 1     | ADR CONTROL |
    /// | 2     | ZERO |
    /// | 3     | Track Number |
    /// | 4     | ZERO |
    /// | 5     | ZERO |
    /// | 6     | ZERO |
    /// | 7     | ZERO |
    /// | 8     | Start Mins |
    /// | 9     | Start Secs |
    /// | 10    | Start Frames |
    ///
    /// # Note
    /// This will also parse the special TOC entries as "tracks"
    /// - 0xA0: first track number = Start Mins
    /// - 0xA1: last track number = Start Mins
    /// - 0xA2: leadout
    pub fn from_scsi_readtoc_0010b(data: &[u8]) -> io::Result<Option<Self>> {
        let _error = tracing::error_span!("TocEntry::from_scsi_readtoc_0010b", data).entered();

        let mut iter = data.iter().copied();

        // After this check it's OK to call `unwrap` on `next`
        (iter.len() == 11)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "invalid 0010b response: expected 11 bytes, received {}",
                        iter.len()
                    ),
                )
            })
            .or_warn("")?;

        let _session_number = iter.next().unwrap();

        let adr_control = iter.next().unwrap();
        let adr = (adr_control & 0b11110000) >> 4;
        if adr != 1 {
            return Ok(None);
        }
        let control = Control::from_bits_truncate(adr_control);

        let tno = iter.next().unwrap();
        (tno == 0)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "invalid TNO"))
            .or_warn("")?;

        let track = iter.next().unwrap();

        let zeros = iter.next_chunk::<4>().unwrap();
        (zeros == [0; _])
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "expected zeros"))
            .or_warn("")?;

        let start = Msf::new(
            iter.next().unwrap(),
            iter.next().unwrap(),
            iter.next().unwrap(),
        );
        let start = Frame::from(start);
        Ok(Some(Self {
            track,
            start,
            control,
        }))
    }

    /// Is an audio track (based on control code & track number)
    pub fn is_audio_track(&self) -> bool {
        self.control.is_audio() && self.track <= 0x63
    }

    /// Is an data track (based on control code & track number)
    pub fn is_data_track(&self) -> bool {
        // TODO: #122 Tracknumbers: types & consts
        self.control.is_data() && self.track <= 0x63
    }

    /// Returns the kind of track (based on control code & track number)
    pub fn kind(&self) -> TocEntryKind {
        match (self.control.is_data(), self.track) {
            (true, ..=0x63) => TocEntryKind::Data,
            (false, ..=0x63) => TocEntryKind::Audio,
            (true, _) => TocEntryKind::DataLeadout,
            (false, _) => TocEntryKind::AudioLeadout,
        }
    }
}

/// The kind of TocEntry. Returned by [TocEntry::kind] and used in [TocEntries::validate_leadouts]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TocEntryKind {
    /// An audio track
    Audio,
    /// A data track
    Data,
    /// The leadout for an audio session
    AudioLeadout,
    /// The leadout for a data session
    DataLeadout,
}

/// CD audio frame (1/75 sec). Basic unit of time for CD audio.
///
/// A frame represents a single unit of CD audio data, which is 1/75th of a second.
/// This is the fundamental unit of time measurement for CD audio.
///
/// # Notes
///
/// - 75 frames = 1 second of audio
/// - Each frame contains 2352 bytes of raw audio data
/// - Used extensively for track positioning and duration calculations
///
/// # Examples
///
/// ```rust
/// use redbook::Frame;
///
/// // Create a frame representing 1 second of audio
/// let one_second = Frame::new(75);
///
/// // Create a frame from a duration
/// use std::time::Duration;
/// let frames = Frame::from(Duration::from_secs(1));
/// assert_eq!(frames.as_usize(), 75);
///
/// ```
///
/// # TODO
/// - impl Display
/// - TODO: #106 Repr transparent, store u-whatever, as_blah casts, etc for Frame, TrackNo, etc...
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Frame(usize);

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
    /// [wiki]: https://en.wikipedia.org/wiki/Compact_Disc_Digital_Audio#Storage_capacity_and_playing_time
    pub const MAX: Self = Frame(usize::MAX);

    /// Creates a new `Frame` from a frame count.
    ///
    /// # Arguments
    ///
    /// * `frames` - The number of CD audio frames
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let five_seconds = Frame::new(75 * 5);
    /// assert_eq!(five_seconds.as_usize(), 375);
    /// ```
    pub const fn new(frames: usize) -> Self {
        Self(frames)
    }

    /// Returns the frame count as a `usize`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let frame = Frame::new(100);
    /// assert_eq!(frame.as_usize(), 100);
    /// ```
    pub const fn as_usize(self) -> usize {
        self.0
    }

    /// Returns a frame relative to the lead-in position.
    ///
    /// This subtracts the standard 150-frame lead-in from the frame position,
    /// giving the position relative to the start of the actual audio data.
    ///
    /// # Returns
    ///
    /// A new `Frame` with the lead-in offset removed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Frame, LEADIN};
    ///
    /// // A frame at position 183 (2 seconds + 33 frames)
    /// let frame = Frame::new(183);
    ///
    /// // Relative to lead-in: 33 frames
    /// let relative = frame.relative_to_leadin();
    /// assert_eq!(relative.as_usize(), 33);
    /// ```
    pub const fn relative_to_leadin(self) -> Self {
        self - LEADIN
    }
}

impl From<Msf> for Frame {
    fn from(msf: Msf) -> Self {
        tracing::trace!(
            target: "frame_conversion",
            min = msf.min,
            sec = msf.sec,
            frame = msf.frame,
            "Frame::from(Msf)"
        );
        Self((((msf.min as usize * 60) + msf.sec as usize) * 75) + msf.frame as usize)
    }
}

impl From<Duration> for Frame {
    fn from(duration: Duration) -> Self {
        tracing::trace!(
            target: "frame_conversion",
            secs = duration.as_secs(),
            "Frame::from(Duration)"
        );
        Msf::from(duration).into()
    }
}

impl From<Frame> for Duration {
    fn from(frames: Frame) -> Self {
        tracing::trace!(
            target: "frame_conversion",
            frames = frames.as_usize(),
            "Duration::from(Frame)"
        );
        Msf::from(frames).into()
    }
}

impl<N> Add<N> for Frame
where
    usize: Add<N, Output = usize>,
{
    type Output = Self;

    fn add(self, rhs: N) -> Self::Output {
        Self(self.0 + rhs)
    }
}

const impl Add<Frame> for Frame {
    type Output = Self;

    fn add(self, rhs: Frame) -> Self::Output {
        Self(self.0 + rhs.0)
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

const impl Sub<Frame> for Frame {
    type Output = Self;

    fn sub(self, rhs: Frame) -> Self::Output {
        Self(self.0 - rhs.0)
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

const impl Mul<Frame> for Frame {
    type Output = Self;

    fn mul(self, rhs: Frame) -> Self::Output {
        Self(self.0 * rhs.0)
    }
}

impl PartialEq<Msf> for Frame {
    fn eq(&self, msf: &Msf) -> bool {
        *self == Self::from(*msf)
    }
}

impl PartialEq<cdtoc::Duration> for Frame {
    fn eq(&self, duration: &cdtoc::Duration) -> bool {
        let (s, f) = duration.seconds_frames();
        self.as_usize().strict_cast::<u64>() == s.strict_mul(75).strict_add(f.strict_cast())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// CD audio duration in min:sec:frame format (75 frames/sec).
///
/// MSF (Minute-Second-Frame) is a common format for representing CD audio positions.
/// Each component is stored as a byte, allowing representation of up to 59 minutes,
/// 59 seconds, and 74 frames (which is slightly less than 60 minutes total).
///
/// # Notes
///
/// - 1 minute = 60 seconds
/// - 1 second = 75 frames
/// - Maximum representable duration: ~59:59.986 (just under 60 minutes)
///
/// # Examples
///
/// ```rust
/// use redbook::{Msf, Frame};
/// use std::time::Duration;
///
/// // Create an MSF value
/// let msf = Msf::new(1, 30, 45); // 1 minute, 30 seconds, 45 frames
///
/// // Convert to a duration
/// let duration = Duration::from(msf);
/// assert_eq!(duration.as_millis(), 90_600);
///
/// // Convert to frames
/// let frames = Frame::from(msf);
/// assert_eq!(frames.as_usize(), 6795);
/// ```
///
///  # TODO
/// - impl Display
pub struct Msf {
    /// Minutes component. This is stored as a u8 in the physical disc TOC.
    min: u8,
    /// Seconds component (0-59).
    sec: u8,
    /// Frames component (0-74).
    frame: u8,
}

impl Msf {
    /// Creates a new `Msf` from minute, second, and frame components.
    ///
    /// # Arguments
    ///
    /// * `min` - Minutes (0-59)
    /// * `sec` - Seconds (0-59)
    /// * `frame` - Frames (0-74)
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Msf;
    ///
    /// let msf = Msf::new(2, 30, 0); // 2 minutes, 30 seconds
    /// ```
    pub const fn new(min: u8, sec: u8, frame: u8) -> Self {
        Self { min, sec, frame }
    }

    /// Returns an MSF value relative to the lead-in position.
    ///
    /// This subtracts the standard 150-frame (2-second) lead-in from the MSF value,
    /// giving the position relative to the start of the actual audio data.
    ///
    /// # Returns
    ///
    /// A new `Msf` with the lead-in offset removed.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Frame, Msf, LEADIN};
    ///
    /// // An MSF at position 0:02:33 (2 seconds + 33 frames = 183 frames)
    /// let msf = Msf::new(0, 2, 33);
    ///
    /// // Relative to lead-in: 0:00:33 (33 frames)
    /// let relative = msf.relative_to_leadin();
    /// assert_eq!(Frame::from(relative), Frame::new(33));
    /// ```
    pub fn relative_to_leadin(self) -> Self {
        self - LEADIN
    }
}

impl Sub<Frame> for Msf {
    type Output = Self;

    fn sub(self, rhs: Frame) -> Self::Output {
        (Frame::from(self) - rhs).into()
    }
}

impl Sub<Duration> for Msf {
    type Output = Self;

    fn sub(self, rhs: Duration) -> Self::Output {
        let as_frames = Frame::from(self) - Frame::from(rhs);
        as_frames.into()
    }
}

impl From<Duration> for Msf {
    /// Converts a [`Duration`] to an [`Msf`].
    ///
    /// # Notes
    ///
    /// - Milliseconds are converted to frames (1000ms = 75 frames)
    /// - The conversion **truncates** fractional frames for safety
    ///
    /// # Example
    ///
    /// ```
    /// # use redbook::Msf;
    /// # use std::time::Duration;
    /// let track1 = Duration::from_millis(180_123); // (3 mins, 9.225 frames)
    /// assert_eq!(Msf::from(track1), Msf::new(3, 0, 9));
    /// let track2 = Duration::from_millis(180_128); // (3mins, 9.6 frames)
    /// assert_eq!(Msf::from(track2), Msf::new(3, 0, 9));
    /// ```
    fn from(duration: Duration) -> Self {
        tracing::trace!(
            target: "frame_conversion",
            secs = duration.as_secs(),
            "Msf::from(Duration)"
        );
        let ms = duration.as_millis();
        let secs = ms / 1000;
        let min = secs / 60;
        let secs = secs % 60;
        let frames = (ms % 1000) * 75 / 1000;
        Self {
            min: min as u8,
            sec: secs as u8,
            frame: frames as u8,
        }
    }
}

impl From<Msf> for Duration {
    /// Converts an [`Msf`] to a [`Duration`].
    ///
    /// # Notes
    ///
    /// - Frames are converted to nanoseconds (1 frame = 1/75 second = ~13_333_333 nanoseconds)
    /// - The conversion uses integer arithmetic for precision
    ///
    /// # Example
    /// ```
    /// # use redbook::Msf;
    /// # use std::time::Duration;
    /// let msf = Msf::new(1,30,30);
    /// assert_eq!(Duration::from(msf), Duration::new(90, 400_000_000))
    /// ```
    fn from(msf: Msf) -> Self {
        let secs = (msf.min * 60) + msf.sec;
        let nanos = msf.frame as u64 * 1_000_000_000 / 75;
        Self::new(secs as u64, nanos as u32)
    }
}

impl From<Frame> for Msf {
    fn from(frames: Frame) -> Self {
        tracing::trace!(
            target: "frame_conversion",
            frames = frames.as_usize(),
            "Msf::from(Frame)"
        );
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

impl PartialEq<Frame> for Msf {
    fn eq(&self, frame: &Frame) -> bool {
        Frame::from(*self) == *frame
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A collection of [`TocEntry`]s, neatly grouped and sorted.
///
/// Construction is available via FromIterator<TocEntry> for io::Result<TocEntries>
pub struct TocEntries {
    entries: Vec<TocEntry>,
    track_types: HashMap<TocEntryKind, usize>,
    leadouts: Leadouts,
}

impl TocEntries {
    /// Provides an iterator over *borrowed* [&TocEntry][TocEntry]s.
    ///
    /// # Note
    ///
    /// [TocEntries] does not directly implement [IntoIterator], but `&TocEntries` does.
    /// Either use `TocEntries.iter()` or `&TocEntries` in a for loop or anywhere else which
    /// expects an `IntoIterator`.
    pub fn iter(&self) -> TocEntriesIter<'_> {
        self.into_iter()
    }

    /// Provides an iterator over the audio tracks as [`Track`]s with duration calculated based
    /// upon the start position of the next track / leadout.
    ///
    /// # TODO
    /// - TODO: #110 handle discs with data before audio, or multiple audio sessions
    pub fn audio_tracks(&self) -> impl Iterator<Item = Track<'static>> {
        const OFFSET_BEFORE_DATA: Frame = Frame::new(11400);

        self.iter()
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then(|| {
                    let mut duration = next_track.start - this_track.start;
                    if next_track.is_data_track() {
                        duration -= OFFSET_BEFORE_DATA
                    }
                    Track::new(this_track.track, this_track.start, duration)
                })
            })
            .flatten()
    }

    /// Get a given *audio* track. Returns `None` if the track number is invalid or not an audio track.
    pub fn track(&self, track: usize) -> Option<Track<'static>> {
        const OFFSET_BEFORE_DATA: Frame = Frame::new(11400);

        self.iter()
            .skip_while(|entry| entry.track.strict_cast::<usize>() != track)
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then(|| {
                    let mut duration = next_track.start - this_track.start;
                    if next_track.is_data_track() {
                        duration -= OFFSET_BEFORE_DATA
                    }
                    Track::new(this_track.track, this_track.start, duration)
                })
            })
            .flatten()
            .next()
    }

    /// Ensures that there is either a leadout between each session, or only one final leadout.
    ///
    /// On success returns the number of sessions identified.
    ///
    /// # Note
    /// - Constructors in this crate ensure that the correct number of leadouts are present. Downstream
    ///   users should not blindly rely on this as foreign crates may incorrectly implement `TryFrom`
    ///   without such validation.
    /// - Many data sources only include information on the final leadout and drop intra-session
    ///   leadouts.
    /// - Discs with multiple audio-sessions are not supported see TODO #110
    pub fn validate_leadouts(&self) -> io::Result<Leadouts> {
        use crate::toc::{Leadouts::*, TocEntryKind::*};
        let sessions = self.data.len() + 1;

        let leadouts: Leadouts = match self.leadouts.len() {
            1 => Leadouts::Final,
            s if s == sessions => Leadouts::PerSession(
                NonZeroUsize::try_from(sessions)
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "no tracks present"))?,
            ),
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "incorrect number of leadouts. Expected 1 or {sessions} found {n}",
                        n = self.leadouts.len()
                    ),
                ));
            }
        };

        self.iter()
            .next()
            .is_some_and(|track| track.is_audio_track() || track.is_data_track())
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "first track is a leadout"))?;

        self.iter().map_windows(|[this_entry, next_entry]| (*this_entry, *next_entry))
        .try_for_each(|(this_entry, next_entry)| {
            match (this_entry.kind(), next_entry.kind(), leadouts) {
                (Audio, Audio, _) => Ok(()),
                (Audio, Data, Final) => Ok(()),
                (Audio, AudioLeadout, _) => Ok(()),
                (Data, DataLeadout, _) => Ok(()),
                (Data, Audio, Final) => Ok(()),
                (AudioLeadout, _, PerSession(_)) => Ok(()),
                (DataLeadout, _, PerSession(_)) => Ok(()),
                _ => Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "invalid toc entry order. {this_entry} cannot be followed by {next_entry}"
                    ),
                )),
            }})?;

        Ok(leadouts)
    }
}

/// Whether only the final leadout is present, or leadouts are available for every session.
///
/// # Interpretation
/// - `Session(1)`: Only one session is present
/// - `Final`: More than one session is present, intra-session leadout positions are unknown
/// - `Session(n)`: More than one session is present,  intra-session leadout positions are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Leadouts {
    /// Multiple sessions exist but only the final leadout is available
    Final,
    /// Every session has a leadout available, includes the number of sessions found.
    PerSession(NonZeroUsize),
}

impl<'entries> IntoIterator for &'entries TocEntries {
    type Item = &'entries TocEntry;

    type IntoIter = TocEntriesIter<'entries>;

    fn into_iter(self) -> Self::IntoIter {
        let audio = self.audio.iter().peekable();
        let data = self.data.iter().peekable();
        let leadouts = self.leadouts.iter().peekable();
        TocEntriesIter {
            audio,
            data,
            leadouts,
        }
    }
}

/// An iterator over the all entries in the order they appear on the CD.
///
/// This `struct` is created by the `into_iter()` method on a *borrowed* [`&TocEntries`][TocEntries]
/// (provided by the [`Iterator`] trait) and the [`iter`][TocEntries::iter] helper method on a
/// [`TocEntries`].
///
/// # TODO
/// - TODO: #123 peekable / peek for TocEntriesIter
pub struct TocEntriesIter<'entries> {
    audio: Peekable<slice::Iter<'entries, TocEntry>>,
    data: Peekable<slice::Iter<'entries, TocEntry>>,
    leadouts: Peekable<slice::Iter<'entries, TocEntry>>,
}

impl<'entries> Iterator for TocEntriesIter<'entries> {
    type Item = &'entries TocEntry;

    fn next(&mut self) -> Option<Self::Item> {
        let next_audio = self
            .audio
            .peek()
            .map(|entry| entry.start)
            .unwrap_or(Frame::MAX);
        let next_data = self
            .data
            .peek()
            .map(|entry| entry.start)
            .unwrap_or(Frame::MAX);
        let next_leadout = self
            .leadouts
            .peek()
            .map(|entry| entry.start)
            .unwrap_or(Frame::MAX);
        let next_start = next_audio.min(next_data).min(next_leadout);

        self.audio
            .next_if(|entry| entry.start == next_start)
            .or(self.data.next_if(|entry| entry.start == next_start))
            .or(self.leadouts.next_if(|entry| entry.start == next_start))
    }
}

impl<'meta, V> PartialEq<V> for TocEntries
where
    for<'v> &'v V: IntoIterator<Item = &'v Track<'meta>>,
{
    fn eq(&self, tracks: &V) -> bool {
        for pair in self.iter().zip_longest(tracks) {
            match pair {
                EitherOrBoth::Both(entry, track) if entry == &track.toc_entry => {}
                _ => {
                    cold_path();
                    return false;
                }
            }
        }
        true
    }
}

impl TryFrom<&TocEntries> for Toc {
    type Error = io::Error;

    fn try_from(entries: &TocEntries) -> Result<Self, Self::Error> {
        // TODO: Don't panic! (via #106)
        // TODO: last() or first()?? Check downstream docs
        // TODO: should this be other or InvalidData?
        let audio = entries
            .audio
            .iter()
            .map(|entry| entry.start.as_usize().strict_cast::<u32>())
            .collect();
        let data = entries
            .data
            .last()
            .map(|entry| entry.start.as_usize().strict_cast::<u32>());
        let leadout = entries
            .leadouts
            .last()
            .map(|entry| entry.start.as_usize().strict_cast::<u32>())
            .unwrap();
        Toc::from_parts(audio, data, leadout).map_err(io::Error::other)
    }
}

impl TryFrom<ReadTocDescriptors> for Toc {
    type Error = io::Error;

    fn try_from(entries: ReadTocDescriptors) -> Result<Self, Self::Error> {
        let organised = &TocEntries::try_from(entries)?;
        organised.try_into()
    }
}

impl FromIterator<TocEntry> for io::Result<TocEntries> {
    fn from_iter<I: IntoIterator<Item = TocEntry>>(iter: I) -> Self {
        use Leadouts::*;
        use TocEntryKind::*;

        let mut entries: Vec<_> = iter.into_iter().collect();

        (entries.len() > 0).ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no TOC entries"))?;

        // INVARIANT: all fields are expected to be sorted by start position
        entries.sort_by_key(|entry| entry.start);

        entries
            .iter()
            .next()
            .is_some_and(|track| track.is_audio_track() || track.is_data_track())
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "first track is a leadout"))?;

        let track_types = entries.iter().map(|entry| entry.kind()).counts();

        let sessions = track_types
            .get(&TocEntryKind::Data)
            .copied()
            .unwrap_or_default()
            + 1;

        let leadout_count = track_types
            .get(&TocEntryKind::DataLeadout)
            .copied()
            .unwrap_or_default()
            + track_types
                .get(&TocEntryKind::AudioLeadout)
                .copied()
                .unwrap_or_default();

        let leadouts = match leadout_count {
            1 => Leadouts::Final,
            s if s == sessions => Leadouts::PerSession(
                NonZeroUsize::try_from(sessions)
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "no tracks present"))?,
            ),
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "incorrect number of leadouts. Expected 1 or {sessions} found {leadout_count}"
                    ),
                ));
            }
        };

        entries.iter().map_windows(|[this_entry, next_entry]| (*this_entry, *next_entry))
        .try_for_each(|(this_entry, next_entry)| {
            match (this_entry.kind(), next_entry.kind(), leadouts) {
                (Audio, Audio, _) => Ok(()),
                (Audio, Data, Final) => Ok(()),
                (Audio, AudioLeadout, _) => Ok(()),
                (Data, DataLeadout, _) => Ok(()),
                (Data, Audio, Final) => Ok(()),
                (AudioLeadout, _, PerSession(_)) => Ok(()),
                (DataLeadout, _, PerSession(_)) => Ok(()),
                _ => Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "invalid toc entry order. {this_entry} cannot be followed by {next_entry}"
                    ),
                )),
            }})?;

        let tocentries = TocEntries {
            entries,
            track_types,
            leadouts,
        };

        Ok(tocentries)
    }
}

impl TryFrom<ReadTocDescriptors> for TocEntries {
    type Error = io::Error;

    fn try_from(descriptors: ReadTocDescriptors) -> Result<Self, Self::Error> {
        descriptors
            .0
            .into_iter()
            .filter_map(|entry| TocEntry::from_scsi_readtoc_0010b(&entry).transpose())
            .try_collect()?
    }
}

/// TOC Track Descriptors as returned by READ_TOC format 0b0010 (See MMC-3 10g 5.23.4)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ReadTocDescriptors(Vec<[u8; 11]>);

impl ReadTocDescriptors {
    /// Errors if not a multiple of 11 bytes
    pub fn new(bytes: Vec<u8>) -> io::Result<Self> {
        let _span = tracing::error_span!("TocString::from_scsi_readtoc_0010b", ?bytes).entered();

        let (entries, rem) = bytes.as_chunks::<11>();
        rem.is_empty()
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    "invalid 0010b output: not multiple of 11 bytes",
                )
            })
            .or_warn("")?;
        let descriptors = entries.to_vec();
        Ok(Self(descriptors))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn leadin_conversion() {
        let dur = Duration::from_secs(2);
        let msf = Msf {
            min: 0,
            sec: 2,
            frame: 0,
        };
        let frames = LEADIN;

        assert_eq!(Msf::from(dur), msf);
        assert_eq!(Msf::from(frames), msf);
        assert_eq!(Frame::from(msf), frames);
        assert_eq!(Frame::from(dur), frames);
        assert_eq!(Duration::from(msf), dur);
        assert_eq!(Duration::from(frames), dur);
    }

    #[test]
    fn leadin_compensation() {
        let starting_frames = Frame::new(183);
        let starting_time = Msf::new(0, 2, 33);
        assert_eq!(starting_frames.relative_to_leadin(), Frame::new(33));
        assert_eq!(starting_time.relative_to_leadin(), Msf::new(0, 0, 33));
    }
}
