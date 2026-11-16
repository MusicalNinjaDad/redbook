//! Physical drives which can read CDs.

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use thread_safely::Context;
use tracing::field::Empty;
use tracing_result::Trace;

pub use error::DriveError;

use crate::{
    Error, Frame, RipProgress, TocEntries, TrackNo, drive::error::DeviceError, frame::ByteSize,
};

#[doc(inline)]
pub use device::Device;

mod device;
pub mod error;

const KB: u32 = 1024;

/// Maximum number of bytes that can be read in a single chunk.
///
/// # Notes
/// - Calculation is via [`MAX_CHUNK_FRAMES`] to ensure sizing for exact number of [`Frame`]s
const MAX_CHUNK_BYTES: ByteSize = MAX_CHUNK_FRAMES.size();

/// Maximum number of frames that can be read in a single chunk.
///
/// # Notes
/// - The Windows API's `IOCTL_CDROM_RAW_READ` has an undocumented maximum chunk size
/// - This value is chosen to stay under a safe buffer size (currently just below 64KB)
///
/// # TODOs
/// - Research the actual maximum chunk size for `IOCTL_CDROM_RAW_READ`
/// - Replace the guessed value (64KB) with a documented reference
const MAX_CHUNK_FRAMES: Frame = Frame::strict_new(
    64_u32
        .strict_mul(KB)
        .strict_div(ByteSize::ONE_FRAME.strict_cast()),
);

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
    fn read_track(&self, track_number: TrackNo) -> Result<Vec<u8>, DriveError> {
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

        let track_size = track.duration().size();

        (track_size > ByteSize::ZERO)
            .ok_or(DriveError::ZeroLengthTrack)
            .or_error("zero length track")?;

        // Vec needs to be initialised to split into chunks. Performance cost insignificant vs IO.
        let mut data = vec![0_u8; track_size.cast::<usize>()];

        let (bufs, last_buf) = data.as_chunks_mut::<{ MAX_CHUNK_BYTES.strict_cast() }>();
        let mut frames_read_so_far = Frame::default();
        trace_span.record("frames_read", format_args!("{frames_read_so_far}"));

        for buf in bufs.iter_mut() {
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

            (frames_read == MAX_CHUNK_FRAMES)
                .ok_or_else(|| DeviceError::UnexpectedEndOfRead {
                    expected: MAX_CHUNK_FRAMES.cast(),
                    received: frames_read.cast(),
                })
                .or_error("wrong number of frames read")?;

            frames_read_so_far = frames_read_so_far
                .try_add(frames_read)
                .or_error("accumulating frames read")?;
            trace_span.record("frames_read", format_args!("{frames_read_so_far}"));
        }

        if !last_buf.is_empty() {
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
                .or_error("calculating starting from for last chunk")?;

            let frames_read = self.device().read_cdda(start, last_buf)?;

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

    /// The underlying device
    fn device(&self) -> &Self::Device;
}

#[derive(Debug, Clone)]
/// TODO: Docs
pub struct CdDrive<DEV: Device> {
    path: PathBuf,
    device: DEV,
    tocentries: TocEntries,
    cx: Context<RipProgress>,
}

impl<DEV: Device> CdDrive<DEV> {
    /// Construct a new CdDrive from constituent parts. You will almost never want to call this directly
    /// unless implementing `From`/`TryFrom`. Use [`open`][Drive::open] instead.
    pub fn new(
        path: PathBuf,
        device: DEV,
        tocentries: TocEntries,
        cx: Context<RipProgress>,
    ) -> Self {
        Self {
            path,
            device,
            tocentries,
            cx,
        }
    }
}

impl<DEV: Device> Drive for CdDrive<DEV> {
    type Device = DEV;

    fn open<P: AsRef<Path> + ?Sized>(
        path: &P,
        cx: Context<RipProgress>,
    ) -> Result<Self, DriveError> {
        let _err_span =
            tracing::error_span!("CdDrive::with_context", path = %path.as_ref().display())
                .entered();
        let _debug_span = tracing::debug_span!("", ?cx).entered();

        let device = DEV::open(&path)?;
        let path = PathBuf::from(path.as_ref());
        let tocentries = device.read_toc()?;
        Ok(Self {
            path,
            device,
            tocentries,
            cx,
        })
    }

    fn cx(&self) -> Context<RipProgress> {
        self.cx.clone()
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn toc(&self) -> &TocEntries {
        &self.tocentries
    }

    fn device(&self) -> &Self::Device {
        &self.device
    }
}

/// TODO: Docs
pub struct CdDrives<DEV: Device> {
    devices: DEV::Iter,
    cx: Context<RipProgress>,
}

impl<DEV: Device> CdDrives<DEV> {
    /// Get an iterator over all available Drives on the current system.
    pub fn all(cx: Context<RipProgress>) -> Result<Self, DriveError> {
        let devices = <CdDrive<DEV> as Drive>::Device::all()?;
        Ok(CdDrives { devices, cx })
    }
}

impl<DEV: Device> Iterator for CdDrives<DEV> {
    /// Drives are yielded as `Result`s so that enumeration & drive-construction failures are
    /// propagated rather than silently ending iteration (issue #50); `None` is reserved for the
    /// expected end-of-enumeration condition.
    type Item = Result<CdDrive<DEV>, Error>;

    fn next(&mut self) -> Option<Self::Item> {
        let _span = tracing::debug_span!("CdDrives::next");
        let device = self.devices.next()?;
        let cx = self.cx.clone();

        let drive = try bikeshed Self::Item {
            let device = device?;
            let path = device.fs_path()?;
            let tocentries = device.read_toc()?;
            CdDrive {
                path,
                device,
                tocentries,
                cx,
            }
        };

        Some(drive)
    }
}

#[cfg(test)]
mod miri {
    use cdtoc::Toc;

    use crate::{Drive, sys::win::DriveHandle, test_fixtures::albums::TestAlbum::*};

    use super::*;

    #[test]
    fn all() {
        let drives = CdDrives::<DriveHandle>::all(Default::default()).unwrap();
        let albums: Vec<_> = drives
            .map(|drive| Toc::try_from(drive.unwrap().toc()).unwrap())
            .collect();
        let expected = [
            DefinitelyMaybe.expected_toc(),
            TheWallDisc1.expected_toc(),
            TheWallDisc2.expected_toc(),
        ];
        assert_eq!(albums, expected);
    }
}
