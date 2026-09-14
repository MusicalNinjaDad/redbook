use std::{
    io::{self, ErrorKind},
    ops::{Add, Rem, Sub},
    time::Duration,
};

use tracing_result::Trace;

use crate::LEADIN;

/// Entry in a CD TOC (Table of Contents).
///
/// Represents a single entry from the CD's Table of Contents, containing the
/// track number and its absolute start position on the disc.
///
/// # Notes
///
/// - The start position includes the lead-in area (150 frames)
/// - Used for low-level disc navigation and track positioning
///
/// # Examples
///
/// ```rust, no_run
/// use redbook::TocEntry;
/// use redbook::Frame;
///
/// // Create a TOC entry for track 1 starting at frame 150 (beginning of lead-in)
/// let entry = TocEntry {
///     track: 1,
///     start: Frame::new(150),
/// };
///
/// assert_eq!(entry.track, 1);
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TocEntry {
    /// The 1-indexed track number.
    pub track: u8,
    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    pub start: Frame,
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
    pub fn from_scsi_readtoc_0010b(data: &[u8]) -> io::Result<Self> {
        dbg!(data);
        let _trace = tracing::trace_span!("TocEntry::from_scsi_readtoc_0010b", data).entered();

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

        let session_number = iter.next().unwrap();
        (session_number == 1)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    "multi-session discs are not supported",
                )
            })
            .or_warn("")?;

        let acr_control = iter.next().unwrap();
        (acr_control & 0b11110000 == 0b00010000)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid ADR value: expected 0001xxxx, got {:#08b}", data[2]),
                )
            })
            .or_warn("")?;

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
        Ok(Self { track, start })
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Frame(usize);

impl Frame {
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

impl<N> Rem<N> for Frame
where
    usize: Rem<N, Output = usize>,
{
    type Output = Self;

    fn rem(self, rhs: N) -> Self::Output {
        Self(self.0 % rhs)
    }
}

impl PartialEq<Msf> for Frame {
    fn eq(&self, msf: &Msf) -> bool {
        *self == Self::from(*msf)
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
    /// Minutes component (0-59).
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

impl PartialEq<Frame> for Msf {
    fn eq(&self, frame: &Frame) -> bool {
        Frame::from(*self) == *frame
    }
}

/// Converts a hex dump of raw TOC data provided by SCSI command READ TOC 0010b to the format
/// `[audio trackcount]+[first audio track address]+[second audio track address]`
/// as used by [cdtoc::Toc::from_cdtoc] and described at
/// [dbpoweramp forum](https://forum.dbpoweramp.com/forum/other-topics/developers-corner/16082-flac-ogg-vorbis-storage-of-cdtoc?16705-FLAC-amp-Ogg-Vorbis-Storage-of-CDTOC=&s=3ca0c65ee58fc45489103bb1c39bfac0&viewfull=1#post76686)
#[tracing::instrument(level = "debug", skip(bytes), fields(entry_count = bytes.len() / 11))]
pub fn parse_toc(bytes: Vec<u8>) -> io::Result<String> {
    let (entries, rem) = bytes.as_chunks::<11>();
    rem.is_empty()
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "invalid 0010b output: not multiple of 11 bytes",
            )
        })
        .or_warn("")?;

    let mut entries: Vec<_> = entries
        .iter()
        .map(|entry| TocEntry::from_scsi_readtoc_0010b(entry.as_slice()))
        .try_collect()?;

    entries.sort_by_key(|entry| entry.track);

    let leadout = entries
        .pop()
        .filter(|leadout| leadout.track == 0xA2)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "no leadout"))?;

    // track number stored in minutes field in TOC
    let Msf {
        min: last_track, ..
    } = entries
        .pop()
        .filter(|last| last.track == 0xA1)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "no last track specified"))?
        .start
        .into();

    let Msf {
        min: first_track, ..
    } = entries
        .pop()
        .filter(|last| last.track == 0xA0)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "no fisrt track specified"))?
        .start
        .into();

    let tracks = entries.len();

    (first_track
        == entries
            .first()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "no tracks"))
            .or_warn("")?
            .track)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "first track number mismatch"))
        .or_warn("")?;

    (last_track
        == entries
            .last()
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "no tracks"))
            .or_warn("")?
            .track)
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "last track number mismatch"))
        .or_warn("")?;

    let timings = entries
        .iter()
        .map(|entry| format!("{frames:02x}+", frames = entry.start.as_usize()))
        .collect::<String>();
    let toc = [
        &format!("{tracks:02x}"),
        timings.trim_end_matches("+"),
        &format!("{:02x}", leadout.start.as_usize()),
    ]
    .join("+");
    Ok(toc)
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
