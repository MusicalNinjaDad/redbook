use std::{
    convert::TryFrom,
    io::{self, ErrorKind},
    ops::Rem,
    sync::Arc,
};

use musicbrainz_rs::entity::discid::Discid;
use tracing::field::Empty;
use tracing_result::Trace;

use crate::{Disc, FRAME_SIZE, MAX_CHUNK_BYTES, MAX_CHUNK_FRAMES, RippedTrack, Track};

/// Trait providing read-only access to audio CD functionality.
///
/// This trait is implemented by types that provide read access to CD audio data,
/// such as [`AudioCd`][crate::AudioCd]. It allows reading raw audio data from tracks and accessing
/// metadata about the disc.
///
/// # Notes
/// - This trait is designed to be used after calling [`lock`](AudioCdExtMut::lock) on
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
    fn disc(&self) -> &Arc<Disc>;

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
    /// - Consider using [`rip`](AudioCdExt::rip) if you need the track number associated with the data
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
    /// - Use [`disc_mut().update_musicbrainz()`](AudioCdExtMut::disc_mut) to fetch MusicBrainz data
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
    /// This is a convenience method that combines [`read_track`](AudioCdExt::read_track)
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
    /// Returns an error if the track cannot be read (see [`read_track`](AudioCdExt::read_track)).
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

    /// Unlock the AudioCd, making it once again !Send and allowing for the underlying Disc to be
    /// mutated.
    /// 
    /// # Note
    /// 
    /// Returns `None` if any references (weak or strong) to the underlying disc are currently open.
    fn unlock(self) -> Option<impl AudioCdExtMut>;
}

/// Trait providing mutable access to audio CD functionality.
///
/// This trait is implemented by types that provide mutable access to CD audio data
/// and metadata, such as [`AudioCd`][crate::AudioCd]. It allows updating metadata and then
/// locking the handle for thread-safe read operations.
///
/// # Notes
///
/// - Use this trait for initial setup: loading MusicBrainz data, selecting releases,
///   and fetching cover art.
/// - After setup, call [`lock`](AudioCdExtMut::lock) to obtain a thread-safe
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
/// cd.disc_mut().set_release_index(Some(2));
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
pub trait AudioCdExtMut: AudioCdExt {
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
    fn disc_mut(&mut self) -> &mut Disc;

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
