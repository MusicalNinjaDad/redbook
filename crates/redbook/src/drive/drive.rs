use std::path::Path;

use thread_safely::Context;
use tracing::field::Empty;
use tracing_result::Trace;

use crate::{
    Device, Frame, RipProgress, TocEntries, TrackNo,
    drive::{DriveError, error::DeviceError},
    frame::ByteSize,
    raw_data::RawData,
};

/// High-level capabilities for CD-drives.
///
/// Compared with [`Device`], implementors of `Drive` can additionally read an entire track and are
/// expected to hold a [`Context<RipProgress>`] to allow for mid-rip termination and status updates.
///
/// Implementors of `Drive` are more likely to store cached versions of data such as their path and
/// ToC, where implementors of only [`Device`] are likely to be low-level entities which directly
/// access hardware.
pub trait Drive: Sized {
    /// The type of [`Device`] which this `Drive` uses to access a disc
    type Device: Device;

    /// Opens a Drive for the given *device path*. See [AudioCd::open][crate::AudioCd::open] for
    /// details of accepted paths.
    ///
    /// Requires a thread [Context] to be provided so that long-running reads via
    /// [read_track][Drive::read_track] can be cancelled and provide status updates.
    fn open<P: AsRef<Path> + ?Sized>(
        path: &P,
        cx: Context<RipProgress>,
    ) -> Result<Self, DriveError>;

    /// Retrieve the thread [Context].
    fn cx(&self) -> Context<RipProgress>;

    /// Read an entire track of data allowing cancellation mid-read and providing status updates
    /// via the thread [Context].
    ///
    /// # Errors
    /// - [`DriveError::NotFound`] if `track_number` does not identify an audio track on the disc
    /// - [`DriveError::ZeroLengthTrack`] if the track has zero duration
    fn read_track(&self, track_number: TrackNo) -> Result<RawData, DriveError> {
        let _err_span = tracing::error_span!("", %track_number).entered();
        let dbg_span = tracing::debug_span!("CdDrive::read_track", track_details = Empty).entered();
        let trace_span = tracing::trace_span!("", frames_read = Empty).entered();

        tracing::info!("reading...");

        let track = self
            .toc()
            .track(track_number)
            .ok_or(DriveError::NotFound(track_number))
            .or_warn("")?;

        dbg_span.record("track_details", format_args!("{track}"));

        (track.duration() > Frame::ZERO)
            .ok_or(DriveError::ZeroLengthTrack)
            .or_error("zero length track")?;

        let mut data = RawData::new(track.duration());

        let chunk_size = self.max_chunk();
        let frames_per_chunk = chunk_size.frames();

        let bufs = data.chunks_mut(chunk_size.strict_cast());
        let last_chunk_index = bufs.len() - 1;

        let mut frames_read_so_far = Frame::default();
        trace_span.record("frames_read", format_args!("{frames_read_so_far}"));

        for (i, buf) in bufs.enumerate() {
            self.cx().cancelled()?;
            self.cx()
                .reply(RipProgress {
                    track_number,
                    processed: frames_read_so_far,
                    total: track.duration(),
                })
                .or_error("sending progress update")?;

            let start = track
                .start()
                .try_add(frames_read_so_far)
                .or_error("calculating starting frame for next chunk")?;

            let frames_read = self.device().read_cdda(start, buf)?;

            ((i == last_chunk_index) || (frames_read == frames_per_chunk))
                .ok_or_else(|| DeviceError::UnexpectedEndOfRead {
                    expected: frames_per_chunk.cast(),
                    received: frames_read.cast(),
                })
                .or_error("wrong number of frames read")?;

            frames_read_so_far = frames_read_so_far
                .try_add(frames_read)
                .or_error("accumulating frames read")?;
            trace_span.record("frames_read", format_args!("{frames_read_so_far}"));
        }

        (frames_read_so_far == track.duration()).ok_or_else(|| {
            DeviceError::UnexpectedEndOfRead {
                expected: track.duration().cast(),
                received: frames_read_so_far.cast(),
            }
        })?;

        self.cx()
            .reply(RipProgress {
                track_number,
                processed: frames_read_so_far,
                total: track.duration(),
            })
            .or_error("sending progress update")?;

        Ok(data)
    }

    /// The path for the drive. If you don't need an owned value then, unlike
    /// [`fs_path`][Device::fs_path], this will avoid a heap allocation and will never error.
    fn path(&self) -> &Path;

    /// The Table of Contents. If you don't need an owned value then, unlike
    /// [`read_toc`][Device::read_toc], this will avoid a heap allocation and will never error.
    fn toc(&self) -> &TocEntries;

    /// The maximum chunk size which can be read in a single call.
    ///
    /// # Note
    /// - Linux has a hard-coded limit of 75 frames
    /// - Windows has a variable limit depending on hardware type
    /// - See #224 for more details / research
    fn max_chunk(&self) -> ByteSize;

    /// The underlying device
    fn device(&self) -> &Self::Device;
}
