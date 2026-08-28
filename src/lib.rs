#![cfg_attr(all(unstable_exact_div, target_family = "windows"), feature(exact_div))]
#![cfg_attr(unstable_exact_size_is_empty, feature(exact_size_is_empty))]
#![cfg_attr(unstable_iter_array_chunks, feature(iter_array_chunks))]
#![cfg_attr(unstable_iterator_try_collect, feature(iterator_try_collect))]
#![cfg_attr(unstable_negative_impls, feature(negative_impls))]
#![cfg_attr(unstable_path_absolute_method, feature(path_absolute_method))]
#![cfg_attr(unstable_try_blocks, feature(try_blocks))]
// Unsafe restricted to dedicated wrapper modules
#![deny(unsafe_code)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![forbid(unsafe_attr_outside_unsafe)]
#![warn(missing_docs)]

//! A load of glue for working with CDDA CD digital audio as per RedBook (IEC 60908:1999)
//!
//! Most of what this library provides is "glue", bringing together crates which cover different
//! parts of the CD audio landscape into a single, coherent whole.
//!
//! # End-to-end functionality
//!
//! 1. **Hardware access** Read audio data from a CD
//! 2. **Parse & lookup** information on the album *including coverart*, generate tags
//!    & embeddable coverart
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
//! // rip the first track, not using threads in this example
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
//! Redbook leverages [tracing](https://crates.io/crates/tracing) with meaningful spans & messages.
//! Specific details are in the documentation for the various modules.
//!
//! # Safety
//!
//! Unsafe code is limited to specific hardware access modules. All unsafe code includes full safety
//! comments.
//!
//! # Thread Safety
//!
//! [AudioCdExt] and [AudioCdExtMut] are separate traits, to allow for initially updating and mutating
//! metadata, before obtaining calling [lock][AudioCdExtMut::lock] so you can use separate threads
//! for reading data and encoding.

pub mod disc;
pub mod hex;
pub mod musicbrainz;
pub mod tagging;
pub mod win;

pub mod test_fixtures;

use tracing::trace;

pub use disc::Disc;
use flacenc::{bitsink::MemSink, component::BitRepr, error::Verify};
pub use win::AudioCd;
use windows_sys::Win32::Devices::Cdrom::TRACK_DATA;

use std::{
    convert::TryFrom,
    io,
    ops::{Add, Rem, Sub},
    sync::Arc,
    time::Duration,
};

use musicbrainz::Discid;

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
    /// an entire track, which may be larger than [`MAX_CHUNK_BYTES`].
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
        let track = self.disc().track(track_number).unwrap();
        tracing::info!(track_number = track.track_number(), "read_track");
        let track_size = track.duration_frames.as_usize().strict_mul(FRAME_SIZE);
        debug_assert!(track_size > 0);

        // Vec needs to be initialised to split into chunks. Performance cost insignificant vs IO.
        let mut data = vec![0_u8; track_size];
        tracing::trace!(data_len = data.len());

        // TODO: Handle very short tracks < MAX_CHUNK_FRAMES
        let (bufs, last_buf) = data.as_chunks_mut::<MAX_CHUNK_BYTES>();
        let mut bytes_read_so_far = 0_i64;

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
        }

        let frame_offset = bufs.len().strict_mul(MAX_CHUNK_FRAMES);
        debug_assert_eq!(
            i64::try_from(frame_offset)
                .unwrap()
                .strict_mul(FRAME_SIZE.try_into().unwrap()),
            bytes_read_so_far,
            "about to read last chunk. We have read {frame_offset} frames, but only {bytes_read_so_far} bytes so far"
        );
        let frames_to_read = track
            .duration_frames
            .as_usize()
            .strict_rem(MAX_CHUNK_FRAMES);

        let bytes_read = self.read_chunk(&track, frame_offset, frames_to_read as u32, last_buf)?;
        bytes_read_so_far += i64::from(bytes_read);

        tracing::trace!(bytes_read_so_far);
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
    ///
    /// // Now safe to use in multiple threads
    /// let handle1 = std::thread::spawn(|| {
    ///     cd.rip(1)
    /// });
    /// let handle2 = std::thread::spawn(|| {
    ///     cd.rip(2)
    /// });
    /// # Ok::<(), io::Error>(())
    /// ```
    fn lock(self) -> impl AudioCdExt + Send;
}

/// A ripped audio track containing raw CD data and metadata.
///
/// This struct wraps the raw audio data from a CD track along with its track number.
/// It provides methods for encoding the raw data to various formats.
///
/// # Notes
///
/// - The raw data is in CDDA format (2352 bytes per frame)
/// - Use [`to_flac`](method@Self::to_flac) or [`to_wav`](method@Self::to_wav) to encode to compressed or uncompressed formats
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
/// // Encode to FLAC
/// let flac_data = track.to_flac();
///
/// // Or encode to WAV
/// let wav_data = track.to_wav();
/// # Ok::<(), io::Error>(())
/// ```
#[derive(Debug, Clone)]
pub struct RippedTrack {
    /// The 1-indexed track number on the original CD.
    pub track_number: usize,
    /// Raw CD audio data (2352 bytes per frame).
    pub raw_data: Vec<u8>,
}

impl RippedTrack {
    /// Encodes the raw CD audio data to FLAC format.
    ///
    /// # Returns
    ///
    /// A [`MemSink<u8>`] containing the FLAC-encoded audio data.
    ///
    /// # Notes
    ///
    /// - The audio is encoded as 16-bit stereo at 44.1 kHz
    /// - FLAC is a lossless compression format, typically reducing CD audio to ~60% of original size
    /// - The returned type can be converted to a byte vector using `.into_inner()`
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
    /// let flac_sink = track.to_flac();
    ///
    /// // Get the FLAC data as bytes
    /// let flac_bytes = flac_sink.into_inner();
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn to_flac(&self) -> MemSink<u8> {
        let (channels, bits_per_sample, sample_rate) = (2, 16, 44100);
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .expect("Config data error.");
        #[expect(
            clippy::chunks_exact_to_as_chunks,
            reason = "TODO error handling if not exact"
        )]
        let samples: Vec<_> = self
            .raw_data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as i32)
            .collect();
        let source = flacenc::source::MemSource::from_samples(
            &samples,
            channels,
            bits_per_sample,
            sample_rate,
        );
        let flac_stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .expect("Encode failed.");
        let mut sink = flacenc::bitsink::ByteSink::new();
        flac_stream.write(&mut sink).unwrap();
        sink
    }

    /// Encodes the raw CD audio data to WAV format.
    ///
    /// # Returns
    ///
    /// A byte vector containing the WAV-encoded audio data.
    ///
    /// # Notes
    ///
    /// - The audio is encoded as 16-bit stereo at 44.1 kHz (standard CD audio)
    /// - WAV is an uncompressed format, so the output will be the same size as the input
    /// - The WAV header is written with the correct RIFF format specifications
    /// - Inspired by implementations from the rust-cd-da-reader project
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// use std::fs::File;
    /// use std::io::Write;
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let cd = AudioCd::new(drive_path)?.lock();
    /// let track = cd.rip(1)?;
    /// let wav_data = track.to_wav();
    ///
    /// // Write to a file
    /// # let _ignore_io_errors = {
    /// let mut file = File::create("track1.wav")?;
    /// file.write_all(&wav_data)?;
    /// # io::Result::Ok(())
    /// # };
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn to_wav(&self) -> Vec<u8> {
        let pcm = &self.raw_data;

        // based on https://github.com/Bloomca/rust-cd-da-reader/blob/fd71208262c199dc44d8a012731be298a848ea79/src/lib.rs#L226
        // & https://github.com/Bloomca/rust-cd-da-reader/blob/main/src/utils.rs#L49
        let pcm_data_size = pcm.len();
        let mut wav = Vec::with_capacity(44 + pcm_data_size);
        let pcm_data_size = pcm_data_size as u32;

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(pcm_data_size + 36).to_le_bytes()); // file size - 8
        wav.extend_from_slice(b"WAVE");

        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
        wav.extend_from_slice(&2u16.to_le_bytes()); // channels
        wav.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
        wav.extend_from_slice(&176400u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&4u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk header
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&pcm_data_size.to_le_bytes());

        wav.extend(pcm);
        wav
    }
}
#[derive(Debug, Clone, PartialEq, Default)]
/// A track on a CD with associated metadata.
///
/// This struct represents a single track on an audio CD, combining low-level
/// TOC (Table of Contents) information with optional metadata from MusicBrainz.
///
/// # Type Parameters
///
/// * `'meta` - Lifetime of the referenced MusicBrainz metadata
///
/// # Notes
///
/// - This type is cheap to clone: all fields are `Copy` except the metadata reference
/// - The borrow checker ensures metadata validity for lifetime `'meta'`
/// - Typically constructed as `<'static>` and then cloned when referencing metadata
/// - The `meta` field is private; use [`meta()`](method@Self::meta) to access it
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
/// if let Some(track) = disc.track(1) {
///     println!("Track {}: {}", track.track_number(), track.title().unwrap_or("Unknown".into()));
///     println!("Filename: {}", track.filename());
/// }
/// # Ok::<(), io::Error>(())
/// ```
pub struct Track<'meta> {
    /// TOC entry for this track, containing track number and start position.
    pub toc_entry: TocEntry,
    /// Duration of this track in frames.
    pub duration_frames: Frame,
    /// Windows-specific track identifier (from CDROM_TOC).
    pub windows_identifier: Option<u32>,
    /// Optional reference to MusicBrainz track metadata.
    meta: Option<&'meta musicbrainz::Track>,
}

impl<'meta> Track<'meta> {
    /// Returns the 1-indexed track number.
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
    /// if let Some(track) = disc.track(1) {
    ///     assert_eq!(track.track_number(), 1);
    /// }
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn track_number(&self) -> u8 {
        self.toc_entry.track
    }

    /// Returns the track title from MusicBrainz metadata, if available.
    ///
    /// # Returns
    ///
    /// `Some(String)` if MusicBrainz metadata is loaded and contains a title,
    /// `None` otherwise.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let mut cd = AudioCd::new(drive_path)?;
    /// // Load MusicBrainz data first
    /// let _ignore_network_errors = cd.disc_mut().update_musicbrainz();
    /// let cd = cd.lock();
    ///
    /// if let Some(track) = cd.disc().track(1) {
    ///     if let Some(title) = track.title() {
    ///         println!("Track 1: {}", title);
    ///     }
    /// }
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn title(&self) -> Option<String> {
        self.meta.map(|track| track.title.clone())
    }

    /// Returns the most likely representation of the track listing, as we expect it was
    /// written on the back of the CD. The only adjustments we make are to ensure that numerical
    /// track numbers are always 2 digits long, in order to allow alphabetical sorting to work.
    ///
    /// # Returns
    ///
    /// A string suitable for use as a filename, e.g., "05 Columbia", "A1 Speak to Me"
    ///
    /// # Notes
    ///
    /// - Uses the text representation for track number from MusicBrainz if available
    /// - Falls back to the two-digit track number if MusicBrainz data is not available
    /// - Numerical track numbers are always formatted as 2 digits (e.g., "05" instead of "5")
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let cd = AudioCd::new(drive_path)?.lock();
    ///
    /// if let Some(track) = cd.disc().track(5) {
    ///     // Will be "05 " followed by the title
    ///     println!("Filename: {}", track.filename());
    /// }
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn filename(&self) -> String {
        let track_num = self
            .meta()
            .map(|trk| {
                let trk_num = trk.number.clone();
                match trk_num.len() {
                    1 if trk_num.parse::<usize>().is_ok() => format!("0{trk_num}"),
                    _ => trk_num,
                }
            })
            .unwrap_or_else(|| format!("{:02}", self.toc_entry.track));
        [track_num, self.title().unwrap_or_default()].join(" ")
    }

    /// Returns a reference to the MusicBrainz track metadata, if available.
    ///
    /// # Returns
    ///
    /// `Some(&musicbrainz::Track)` if MusicBrainz metadata is loaded for this track,
    /// `None` otherwise.
    ///
    /// # Notes
    ///
    /// - This provides access to the full MusicBrainz track data, including artist, album, etc.
    /// - The lifetime of the returned reference is tied to the `'meta` lifetime parameter
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
    /// # use std::{io, path::PathBuf};
    /// # let drive_path = PathBuf::new();
    ///
    /// let mut cd = AudioCd::new(drive_path)?;
    /// // Load MusicBrainz data first
    /// let _ignore_network_errors = cd.disc_mut().update_musicbrainz();
    /// let cd = cd.lock();
    ///
    /// if let Some(track) = cd.disc().track(1) {
    ///     if let Some(meta) = track.meta() {
    ///         // Access full MusicBrainz metadata
    ///         println!("Artist: {:?}", meta.artist_credit);
    ///     }
    /// }
    /// # Ok::<(), io::Error>(())
    /// ```
    pub fn meta(&self) -> Option<&'meta musicbrainz::Track> {
        self.meta
    }
}

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
/// ```
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

impl From<&TRACK_DATA> for TocEntry {
    /// Creates a [`TocEntry`] from Windows API CDROM_TRACK_DATA.
    ///
    /// # Arguments
    ///
    /// * `track_data` - Raw track data from the Windows CDROM_TOC
    ///
    /// # Notes
    ///
    /// - The address is read as big-endian and converted to a frame position
    /// - The lead-in offset is added to get the absolute frame position
    ///
    /// # TODOs
    ///
    /// - Consider making this fallible with `TryFrom` for better error handling
    fn from(track_data: &TRACK_DATA) -> Self {
        let relative = u32::from_be_bytes(track_data.Address);
        let start = Frame::new(relative as usize) + LEADIN;
        let track = track_data.TrackNumber;
        Self { track, start }
    }
}

impl From<Msf> for Frame {
    /// Converts an [`Msf`] (min:sec:frame) value to a [`Frame`].
    ///
    /// # Arguments
    ///
    /// * `msf` - The MSF value to convert
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Frame, Msf};
    ///
    /// let msf = Msf::new(1, 30, 0); // 1 minute 30 seconds
    /// let frame = Frame::from(msf);
    ///
    /// // 1 minute 30 seconds = (60 + 30) * 75 = 6750 frames
    /// assert_eq!(frame.as_usize(), 6750);
    /// ```
    fn from(msf: Msf) -> Self {
        trace!(
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
    /// Converts a [`Duration`] to a [`Frame`].
    ///
    /// # Arguments
    ///
    /// * `duration` - The duration to convert
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    /// use std::time::Duration;
    ///
    /// let duration = Duration::from_secs(2);
    /// let frame = Frame::from(duration);
    ///
    /// // 2 seconds = 2 * 75 = 150 frames
    /// assert_eq!(frame.as_usize(), 150);
    /// ```
    fn from(duration: Duration) -> Self {
        trace!(
            target: "frame_conversion",
            secs = duration.as_secs(),
            "Frame::from(Duration)"
        );
        Msf::from(duration).into()
    }
}

impl From<Frame> for Duration {
    /// Converts a [`Frame`] to a [`Duration`].
    ///
    /// # Arguments
    ///
    /// * `frames` - The frame to convert
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    /// use std::time::Duration;
    ///
    /// let frame = Frame::new(150); // 2 seconds
    /// let duration = Duration::from(frame);
    ///
    /// assert_eq!(duration, Duration::from_secs(2));
    /// ```
    fn from(frames: Frame) -> Self {
        trace!(
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

    /// Adds a value to this frame.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let frame = Frame::new(100);
    /// let result = frame + 50;
    ///
    /// assert_eq!(result.as_usize(), 150);
    /// ```
    fn add(self, rhs: N) -> Self::Output {
        Self(self.0 + rhs)
    }
}

impl Add<Frame> for Frame {
    type Output = Self;

    /// Adds two frames together.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let frame1 = Frame::new(100);
    /// let frame2 = Frame::new(50);
    /// let result = frame1 + frame2;
    ///
    /// assert_eq!(result.as_usize(), 150);
    /// ```
    fn add(self, rhs: Frame) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl Sub<Frame> for Frame {
    type Output = Self;

    /// Subtracts one frame from another.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let frame1 = Frame::new(100);
    /// let frame2 = Frame::new(50);
    /// let result = frame1 - frame2;
    ///
    /// assert_eq!(result.as_usize(), 50);
    /// ```
    fn sub(self, rhs: Frame) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl<N> Sub<N> for Frame
where
    usize: Sub<N, Output = usize>,
{
    type Output = Self;

    /// Subtracts a value from this frame.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let frame = Frame::new(100);
    /// let result = frame - 50;
    ///
    /// assert_eq!(result.as_usize(), 50);
    /// ```
    fn sub(self, rhs: N) -> Self::Output {
        Self(self.0 - rhs)
    }
}

impl<N> Rem<N> for Frame
where
    usize: Rem<N, Output = usize>,
{
    type Output = Self;

    /// Returns the remainder of dividing this frame by a value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Frame;
    ///
    /// let frame = Frame::new(100);
    /// let result = frame % 30;
    ///
    /// assert_eq!(result.as_usize(), 10);
    /// ```
    fn rem(self, rhs: N) -> Self::Output {
        Self(self.0 % rhs)
    }
}

impl PartialEq<Msf> for Frame {
    /// Compares this frame for equality with an [`Msf`] value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Frame, Msf};
    ///
    /// let frame = Frame::new(150);
    /// let msf = Msf::new(0, 2, 0); // 2 seconds = 150 frames
    ///
    /// assert_eq!(frame, msf);
    /// ```
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
/// assert!(duration.as_secs() > 90);
///
/// // Convert to frames
/// let frames = Frame::from(msf);
/// assert!(frames.as_usize() > 6750);
/// ```
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
    /// use redbook::{Msf, LEADIN};
    ///
    /// // An MSF at position 0:02:33 (2 seconds + 33 frames = 183 frames)
    /// let msf = Msf::new(0, 2, 33);
    ///
    /// // Relative to lead-in: 0:00:33 (33 frames)
    /// let relative = msf.relative_to_leadin();
    /// assert_eq!(relative.frame, 33);
    /// assert_eq!(relative.sec, 0);
    /// assert_eq!(relative.min, 0);
    /// ```
    pub fn relative_to_leadin(self) -> Self {
        self - LEADIN
    }
}

impl Sub<Frame> for Msf {
    type Output = Self;

    /// Subtracts a frame from this MSF value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Msf, Frame};
    ///
    /// let msf = Msf::new(1, 0, 0); // 1 minute
    /// let result = msf - Frame::new(30); // Subtract 30 frames
    ///
    /// // 1 minute = 4500 frames, 4500 - 30 = 4470 frames = 59 seconds 45 frames
    /// assert_eq!(result.min, 0);
    /// assert_eq!(result.sec, 59);
    /// assert_eq!(result.frame, 45);
    /// ```
    fn sub(self, rhs: Frame) -> Self::Output {
        (Frame::from(self) - rhs).into()
    }
}

impl Sub<Duration> for Msf {
    type Output = Self;

    /// Subtracts a duration from this MSF value.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Msf;
    /// use std::time::Duration;
    ///
    /// let msf = Msf::new(1, 0, 0); // 1 minute
    /// let result = msf - Duration::from_secs(30); // Subtract 30 seconds
    ///
    /// assert_eq!(result.min, 0);
    /// assert_eq!(result.sec, 30);
    /// assert_eq!(result.frame, 0);
    /// ```
    fn sub(self, rhs: Duration) -> Self::Output {
        let as_frames = Frame::from(self) - Frame::from(rhs);
        as_frames.into()
    }
}

impl From<Duration> for Msf {
    /// Converts a [`Duration`] to an [`Msf`].
    ///
    /// # Arguments
    ///
    /// * `duration` - The duration to convert
    ///
    /// # Notes
    ///
    /// - Milliseconds are converted to frames (1000ms = 75 frames)
    /// - The conversion truncates fractional frames
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Msf;
    /// use std::time::Duration;
    ///
    /// let duration = Duration::from_secs(90); // 1 minute 30 seconds
    /// let msf = Msf::from(duration);
    ///
    /// assert_eq!(msf.min, 1);
    /// assert_eq!(msf.sec, 30);
    /// assert_eq!(msf.frame, 0);
    /// ```
    fn from(duration: Duration) -> Self {
        trace!(
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
    /// # Arguments
    ///
    /// * `msf` - The MSF value to convert
    ///
    /// # Notes
    ///
    /// - Frames are converted to nanoseconds (1 frame = 1/75 second = ~13333333 nanoseconds)
    /// - The conversion uses integer arithmetic for precision
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::Msf;
    /// use std::time::Duration;
    ///
    /// let msf = Msf::new(1, 30, 0); // 1 minute 30 seconds
    /// let duration = Duration::from(msf);
    ///
    /// assert_eq!(duration, Duration::from_secs(90));
    /// ```
    fn from(msf: Msf) -> Self {
        let secs = (msf.min * 60) + msf.sec;
        let nanos = msf.frame as u32 * 75 / 1_000_000_000;
        Self::new(secs as u64, nanos)
    }
}

impl From<Frame> for Msf {
    /// Converts a [`Frame`] to an [`Msf`].
    ///
    /// # Arguments
    ///
    /// * `frames` - The frame to convert
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Frame, Msf};
    ///
    /// let frames = Frame::new(6750); // 6750 frames = 90 seconds = 1 minute 30 seconds
    /// let msf = Msf::from(frames);
    ///
    /// assert_eq!(msf.min, 1);
    /// assert_eq!(msf.sec, 30);
    /// assert_eq!(msf.frame, 0);
    /// ```
    fn from(frames: Frame) -> Self {
        trace!(
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
    /// Compares this MSF value for equality with a [`Frame`].
    ///
    /// # Examples
    ///
    /// ```rust
    /// use redbook::{Frame, Msf};
    ///
    /// let msf = Msf::new(0, 2, 0); // 2 seconds = 150 frames
    /// let frames = Frame::new(150);
    ///
    /// assert_eq!(msf, frames);
    /// ```
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
