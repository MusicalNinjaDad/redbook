// Unsafe restricted to dedicated wrapper modules
#![deny(unsafe_code)]
// Only applicable to library
#![deny(missing_docs)]
// All experimental features are only enabled when on a toolchain where they are still unstable
#![cfg_attr(unstable_const_trait_impl, feature(const_trait_impl))]
#![cfg_attr(unstable_default_field_values, feature(default_field_values))]
#![cfg_attr(unstable_exact_size_is_empty, feature(exact_size_is_empty))]
#![cfg_attr(unstable_integer_casts, feature(integer_casts))]
#![cfg_attr(unstable_integer_cast_extras, feature(integer_cast_extras))]
#![cfg_attr(unstable_iter_array_chunks, feature(iter_array_chunks))]
#![cfg_attr(unstable_iter_next_chunk, feature(iter_next_chunk))]
#![cfg_attr(unstable_iterator_try_collect, feature(iterator_try_collect))]
#![cfg_attr(unstable_negative_impls, feature(negative_impls))]
#![cfg_attr(unstable_path_absolute_method, feature(path_absolute_method))]
#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]

//! A load of glue for working with CDDA CD digital audio as per RedBook (IEC 60908:1999)
//!
//! Most of what this library provides is "glue", bringing together crates which cover different
//! parts of the CD audio landscape into a single, coherent whole.
//!
//! # End-to-end functionality
//!
//! 1. **Hardware access** Read audio data from a CD
//! 2. **Parse & lookup** information on the album, generate tags  & embeddable coverart
//! 3. **Encode music** to wav or flac
//!
//! # Structure
//!
//! There are 3 key entry points to the crate, one each for hardware, CD structure, and music data.
//!
//! - [AudioCd] & [AudioCdExt]  - for interfacing with hardware
//! - [Disc]                    - for working with the contents of a CD
//! - [RippedTrack]             - for the actual music of a given track
//!
//! # Example
//!
//! ```rust, no_run
//! use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
//! # use std::{io, path::PathBuf};
//! # use metaflac::block::{Picture, VorbisComment};
//! # let drive_path = PathBuf::new();
//!
//! // Open a handle to the drive and read table of contents from the CD
//! let mut cd: AudioCd = AudioCd::new(drive_path)?;
//!
//! // Try to get data on this cd from musicbrainz. Continue on (network) errors.
//! let _ = cd.disc_mut().update_musicbrainz();
//!
//! // There are often multiple releases with the same tracks - select the right one.
//! cd.disc_mut().set_release(Some(2));
//!
//! // Try to get the cover art from CoverArtArchive based on the musicbrainz info.
//! let _ = cd.disc_mut().update_cover_art();
//!
//! // Make the AudioCd immutable, so we can safely spawn separate threads to rip & encode data.
//! let cd = cd.lock();
//!
//! // See bin/rip/main.rs for an example of how to use channels & separate threads to rip & encode.
//!
//! // rip the first track
//! let track1 = cd.rip(1)?;
//!
//! // encode the first track to flac
//! let track1_flac = track1.to_flac();
//!
//! // get the tags & embeddable cover art
//! let tags: Option<VorbisComment> = cd.disc().tag_for(1);
//! let cover: Option<&Picture> = cd.disc().cover_art();
//! # Ok::<(), io::Error>(())
//! ```
//!
//! # Tracing
//!
//! Redbook leverages [tracing](https://crates.io/crates/tracing). Info, Warn & Error messages
//! are designed to be directly usable as output from a CLI binary.
//!
//! # Safety
//!
//! - Unsafe code is limited to specific hardware access modules.
//! - `#![deny(unsafe_code)]` with a wide selection of additional lints defined in `Cargo.toml`
//! - All other modules are marked `#[forbid(unsafe_code)]`.
//! - Every unsafe call is annotated with `#[expect(unsafe_code, reason = "...")]`.
//! - All unsafe code includes full safety comments.
//! - All ffi calls are also mocked with full safety instructions, ensuring that IDE integration
//!   provides these details in-situ. The correctness of the mock signatures is validated on every
//!   test run.
//! - We use miri to check for potential UB (thanks to the mocks we can create tests for miri to run
//!   that validate all the unsafe callsites)
//! - It goes without saying but, ALL unsafe code is *hand crafted by humans*. Agents.md
//!   specifically forbids any unsafe code changes or generation.
//!
//! # Thread Safety
//!
//! File handles are not `Sync`, but you almost certainly will want to split reading data from a CD
//! and processing that data into separate threads. To facilitate this [AudioCdExt] and
//! [AudioCdExtMut] are separate traits. See the example for how to take advantage to initially
//! update and mutate metadata, before obtaining calling [lock][AudioCdExtMut::lock] and spawning
//! threads to use that data.
//!
//! # Nightly only
//!
//! This crate is nightly only for a few reasons:
//! - I want to rely on downstream crates which use nightly features, in particular: leveraging
//!   `poratble_simd` in [`flacenc`] (currently disabled); and `try_trait_v2` for [`exit_safely`] in
//!   binaries & tracing ergonomics via [`tracing_result`].
//! - I find many of the ergonomic benefits worth the toolchain restriction.
//! - I want to support development of the language and stabilisation of new features.
//!
//! The crate uses [`build_safely`] to ensure that every experimental feature behaves as expected and
//! to avoid future lint errors for stable features

#[forbid(unsafe_code)]
pub mod disc;
#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod hex;
#[forbid(unsafe_code)]
mod track;
pub use track::*;

#[forbid(unsafe_code)]
pub mod tagging;

// provides abstractions over direct hardware access
pub mod win;

#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod test_fixtures;

pub use disc::Disc;
use musicbrainz_rs::entity::discid::Discid;
use tracing::field::Empty;
use tracing_result::Trace;
pub use win::AudioCd;

use std::{
    convert::TryFrom,
    io::{self, ErrorKind},
    ops::{Add, Rem, Sub},
    sync::Arc,
    time::Duration,
};

/// Size of a single CDDA audio frame in bytes.
///
/// According to the RedBook standard (IEC 60908:1999), each frame contains 2352 bytes
/// of raw audio data.
const FRAME_SIZE: usize = 2352;

/// Maximum number of frames that can be read in a single chunk.
///
/// # Notes
/// - The Windows API's `IOCTL_CDROM_RAW_READ` has an undocumented maximum chunk size
/// - This value is calculated to stay under a safe buffer size (currently 64KB)
///
/// # TODOs
/// - Research the actual maximum chunk size for `IOCTL_CDROM_RAW_READ`
/// - Replace the guessed value (64KB) with a documented reference
const MAX_CHUNK_FRAMES: usize = 64 * 1024 / FRAME_SIZE;

/// Maximum number of bytes that can be read in a single chunk.
///
/// Calculated as [`MAX_CHUNK_FRAMES`] * [`FRAME_SIZE`].
const MAX_CHUNK_BYTES: usize = MAX_CHUNK_FRAMES * FRAME_SIZE;

/// Standard CD lead-in duration in frames.
///
/// According to the RedBook standard, CD audio has a 2-second lead-in area
/// at the beginning of the disc. At 75 frames per second, this equals 150 frames.
///
/// This is used as an offset when calculating absolute frame positions on the disc.
pub const LEADIN: Frame = Frame(150);

/// Trait providing read-only access to audio CD functionality.
///
/// This trait is implemented by types that provide read access to CD audio data,
/// such as [`AudioCd`]. It allows reading raw audio data from tracks and accessing
/// metadata about the disc.
///
/// # Notes
/// - This trait is designed to be used after calling [`lock`](trait@AudioCdExtMut::lock) on
///   a mutable handle, ensuring thread-safe access to the CD.
/// - All methods are safe and do not require unsafe code.
///
/// # Examples
///
/// ```rust, no_run
/// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
/// # use std::{io, path::PathBuf};
/// # let drive_path = PathBuf::new();
///
/// // First obtain a mutable handle and lock it for thread-safe reading
/// let mut cd = AudioCd::new(drive_path)?;
/// let cd = cd.lock();
///
/// // Now you can use AudioCdExt methods
/// let disc = cd.disc();
/// let track_data = cd.read_track(1)?;
/// # Ok::<(), io::Error>(())
/// ```
pub trait AudioCdExt {
    /// Reads raw audio data from a specific track and frame offset.
    ///
    /// # Arguments
    ///
    /// * `track` - The track to read from
    /// * `frame_offset` - Offset in frames from the start of the track
    /// * `frames_to_read` - Number of frames to read
    /// * `buf` - Buffer to read data into
    ///
    /// # Returns
    ///
    /// The number of bytes read into the buffer.
    ///
    /// # Errors
    ///
    /// Returns an error if the read operation fails, typically due to:
    /// - Hardware communication errors
    /// - Invalid track or frame offset
    /// - Buffer too small for the requested data
    fn read_chunk(
        &self,
        track: &Track,
        frame_offset: usize,
        frames_to_read: u32,
        buf: &mut [u8],
    ) -> io::Result<u32>;

    /// Returns a reference to the cached [`Disc`] data.
    ///
    /// The disc data includes track listings, durations, and any metadata
    /// that has been loaded (such as MusicBrainz information).
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let cd = AudioCd::new(drive_path)?.lock();
    /// let disc = cd.disc();
    ///
    /// // Access track information
    /// let num_tracks = disc.tracks().len();
    /// # Ok::<(), io::Error>(())
    /// ```
    fn disc(&self) -> &Arc<crate::Disc>;

    /// Reads all frames from a track and returns the raw audio data.
    ///
    /// This is a convenience method that handles the chunking logic for reading
    /// an entire track, which may be larger than can be read in a single IO call.
    ///
    /// # Arguments
    ///
    /// * `track_number` - The 1-indexed track number to read
    ///
    /// # Returns
    ///
    /// A vector containing the raw CD audio data (2352 bytes per frame).
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The track number is invalid
    /// - Any read operation fails
    ///
    /// # Notes
    ///
    /// - The returned data is in raw CDDA format (2352 bytes per frame)
    /// - For a typical 4-minute song, this will be approximately 40-50 MB
    /// - Consider using [`rip`](trait@AudioCdExt::rip) if you need the track number associated with the data
    fn read_track(&self, track_number: usize) -> io::Result<Vec<u8>> {
        let _warn = tracing::warn_span!("read track", track_number).entered();
        let trace =
            tracing::trace_span!("read track", track_size = Empty, bytes_read = Empty).entered();

        tracing::info!("");

        let track = self
            .disc()
            .track(track_number)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "invalid track number"))
            .or_warn("")?;

        let track_size = track
            .duration
            .as_usize()
            .checked_mul(FRAME_SIZE)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::FileTooLarge,
                    format!(
                        "track too long. {size} bytes but can only handle {max}",
                        size = (track.duration.as_usize() as u128) * (FRAME_SIZE as u128),
                        max = usize::MAX
                    ),
                )
            })
            .or_error("")?;

        trace.record("track_size", track_size);

        (track_size > 0)
            .ok_or_else(|| io::Error::new(ErrorKind::UnexpectedEof, "zero length track"))
            .or_error("")?;

        // Vec needs to be initialised to split into chunks. Performance cost insignificant vs IO.
        let mut data = vec![0_u8; track_size];

        let (bufs, last_buf) = data.as_chunks_mut::<MAX_CHUNK_BYTES>();
        let mut bytes_read_so_far = 0_i64;
        trace.record("bytes_read", bytes_read_so_far);

        for (i, buf) in bufs.iter_mut().enumerate() {
            let frames_to_read: u32 = MAX_CHUNK_FRAMES.try_into().unwrap();

            debug_assert_eq!(
                bytes_read_so_far,
                (i as i64).strict_mul(MAX_CHUNK_BYTES as i64),
                "now reading chunk {i} but have only read {bytes_read_so_far} bytes so far"
            );

            let frame_offset = i * MAX_CHUNK_FRAMES;
            debug_assert_eq!(
                i64::try_from(frame_offset)
                    .unwrap()
                    .strict_mul(FRAME_SIZE.try_into().unwrap()),
                bytes_read_so_far,
                "about to read chunk {i}. We have read {frame_offset} frames, but only {bytes_read_so_far} bytes so far"
            );

            let bytes_read = self.read_chunk(&track, frame_offset, frames_to_read, buf)?;
            bytes_read_so_far += i64::from(bytes_read);
            trace.record("bytes_read", bytes_read_so_far);
        }

        // Frames are multiple bytes, therefore must fit in usize, if track_size does
        let frame_offset = bufs.len() * MAX_CHUNK_FRAMES;
        debug_assert_eq!(
            i64::try_from(frame_offset)
                .unwrap()
                .strict_mul(FRAME_SIZE.try_into().unwrap()),
            bytes_read_so_far,
            "about to read last chunk. We have read {frame_offset} frames, but only {bytes_read_so_far} bytes so far"
        );

        let frames_to_read = track.duration.as_usize().rem(MAX_CHUNK_FRAMES);
        debug_assert_eq!(frames_to_read * FRAME_SIZE, last_buf.len());

        if !last_buf.is_empty() {
            let bytes_read =
                self.read_chunk(&track, frame_offset, frames_to_read as u32, last_buf)?;
            bytes_read_so_far += i64::from(bytes_read);
            trace.record("bytes_read", bytes_read_so_far);
        }

        Ok(data)
    }

    /// Returns cached MusicBrainz data for this disc, if available.
    ///
    /// MusicBrainz data includes album information, track listings, and metadata
    /// that can be used for tagging ripped tracks.
    ///
    /// # Returns
    ///
    /// `Some(&Discid)` if MusicBrainz data has been successfully loaded,
    /// `None` otherwise.
    ///
    /// # Notes
    ///
    /// - Use [`disc_mut().update_musicbrainz()`](trait@AudioCdExtMut::disc_mut) to fetch MusicBrainz data
    /// - The data is cached in the [`Disc`] struct
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let cd = AudioCd::new(drive_path)?.lock();
    /// if let Some(discid) = cd.musicbrainz() {
    ///     println!("Disc ID: {}", discid.id);
    /// }
    /// # Ok::<(), io::Error>(())
    /// ```
    fn musicbrainz(&self) -> Option<&Discid> {
        self.disc().musicbrainz()
    }

    /// Rips a single track, returning track metadata and raw audio data.
    ///
    /// This is a convenience method that combines [`read_track`](trait@AudioCdExt::read_track)
    /// with track number information, returning a [`RippedTrack`] struct.
    ///
    /// # Arguments
    ///
    /// * `track_number` - The 1-indexed track number to rip
    ///
    /// # Returns
    ///
    /// A [`RippedTrack`] containing the track number and raw audio data.
    ///
    /// # Errors
    ///
    /// Returns an error if the track cannot be read (see [`read_track`](trait@AudioCdExt::read_track)).
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let cd = AudioCd::new(drive_path)?.lock();
    /// let track = cd.rip(1)?;
    ///
    /// println!("Ripped track {} ({} bytes)", track.track_number, track.raw_data.len());
    /// # Ok::<(), io::Error>(())
    /// ```
    fn rip(&self, track_number: usize) -> io::Result<RippedTrack> {
        let raw_data = self.read_track(track_number)?;
        Ok(RippedTrack {
            track_number,
            raw_data,
        })
    }
}

/// Trait providing mutable access to audio CD functionality.
///
/// This trait is implemented by types that provide mutable access to CD audio data
/// and metadata, such as [`AudioCd`]. It allows updating metadata and then
/// locking the handle for thread-safe read operations.
///
/// # Notes
///
/// - Use this trait for initial setup: loading MusicBrainz data, selecting releases,
///   and fetching cover art.
/// - After setup, call [`lock`](trait@AudioCdExtMut::lock) to obtain a thread-safe
///   immutable handle implementing [`AudioCdExt`].
///
/// # Examples
///
/// ```rust, no_run
/// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
/// # use std::{io, path::PathBuf};
/// # let drive_path = PathBuf::new();
///
/// // Obtain a mutable handle for setup
/// let mut cd = AudioCd::new(drive_path)?;
///
/// // Update metadata from MusicBrainz
/// let _ignore_network_errors = cd.disc_mut().update_musicbrainz();
///
/// // Select a specific release
/// cd.disc_mut().set_release(Some(2));
///
/// // Fetch cover art
/// let _ignore_network_errors = cd.disc_mut().update_cover_art();
///
/// // Lock for thread-safe reading
/// let cd = cd.lock();
///
/// // Now use AudioCdExt methods
/// let track = cd.rip(1)?;
/// # Ok::<(), io::Error>(())
/// ```
pub trait AudioCdExtMut {
    /// Returns a mutable reference to the cached [`Disc`] data.
    ///
    /// This allows modification of disc metadata, such as loading MusicBrainz
    /// information or selecting a specific release.
    ///
    /// # Returns
    ///
    /// A mutable reference to the disc data.
    ///
    /// # Notes
    ///
    /// - The disc is backed by an [`Arc`], so this method uses `Arc::make_mut` internally
    /// - If other references to the disc exist, this will clone the internal data
    /// - This is a cheap operation for the metadata, but be aware of the semantics
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let mut cd = AudioCd::new(drive_path)?;
    ///
    /// // Update MusicBrainz data
    /// let _ignore_network_errors = cd.disc_mut().update_musicbrainz();
    ///
    /// // Access the mutable disc to make changes
    /// let disc = cd.disc_mut();
    /// // ... modify disc as needed
    /// # Ok::<(), io::Error>(())
    /// ```
    fn disc_mut(&mut self) -> &mut crate::disc::Disc;

    /// Consumes self and returns an immutable, thread-safe handle.
    ///
    /// This method transforms the mutable handle into an immutable one that
    /// implements [`AudioCdExt`] and [`Send`], allowing it to be safely shared
    /// across threads.
    ///
    /// # Returns
    ///
    /// An immutable handle that can be safely shared across threads.
    ///
    /// # Notes
    ///
    /// - After calling this method, you can no longer mutate the disc metadata
    /// - The returned handle is suitable for spawning threads to rip and encode tracks in parallel
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let mut cd = AudioCd::new(drive_path)?;
    ///
    /// // Perform setup
    /// let _ignore_network_errors = cd.disc_mut().update_musicbrainz();
    ///
    /// // Lock for thread-safe access
    /// let cd = cd.lock();
    /// let disc = cd.disc().clone();
    ///
    /// // Now safe to use cd & disc in multiple threads
    /// let ripper_thread = std::thread::spawn(move || {
    ///     cd.rip(1)
    /// });
    /// let metadata_thread = std::thread::spawn(move || {
    ///     let _ = disc.musicbrainz();
    /// });
    /// # Ok::<(), io::Error>(())
    /// ```
    fn lock(self) -> impl AudioCdExt + Send;
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
    pub fn new(frames: usize) -> Self {
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
    pub fn as_usize(self) -> usize {
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
    pub fn relative_to_leadin(self) -> Self {
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

impl Add<Frame> for Frame {
    type Output = Self;

    fn add(self, rhs: Frame) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub<Frame> for Frame {
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
    pub fn new(min: u8, sec: u8, frame: u8) -> Self {
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
