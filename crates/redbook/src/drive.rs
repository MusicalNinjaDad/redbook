//! Physical drives which can read CDs

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use thread_safely::Context;
use tracing::field::Empty;
use tracing_result::Trace;

pub use error::DriveError;

use crate::{
    Error, Frame, MAX_CHUNK_BYTES, MAX_CHUNK_FRAMES, RipProgress, TocEntries, TrackNo,
    drive::error::{DeviceError, OsError},
    frame::ByteSize,
};

pub mod error;
/// The capabilites required of a low level device which can read CDs
pub trait Device: Sized {
    /// The coloured io::Error returned by this device.
    ///
    /// This allows for OS-specific parsing of non-overlapping error codes
    /// (e.g. windows `ERROR_NOT_READY` == 21, vs linux `ENOMEDIUM` == 123)
    type OsError: OsError;

    /// The type of Iterator available that provides all available devices on the system.
    ///
    /// Items are `Result`s so that enumeration / drive-construction failures are propagated
    /// rather than silently ending iteration (issue #50): `None` is reserved for the expected
    /// end-of-enumeration condition.
    type Iter: Iterator<Item = Result<Self, DeviceError>>;

    /// Open the device represented by the path. See [AudioCd::open][crate::AudioCd::open] for
    /// details on acceptable path formats.
    fn open<P: AsRef<Path> + ?Sized>(path: &P) -> Result<Self, DeviceError>;

    /// Get an iterator over all available cd devices on the system
    fn all() -> Result<Self::Iter, DeviceError>;

    /// Get a valid file system-path which represents the device and can be passed to
    /// [`std::fs::read_dir`] etc. to read from the device as a file-system.
    ///
    /// Depending on the implementation this may not be the same value as was provided to `open`.
    /// E.g. on windows this could return `\\.\CDROM0` when `F:` was passed to `open`.
    fn fs_path(&self) -> Result<PathBuf, DeviceError>;

    /// Read raw CDDA data starting at frame `start` into `buf`
    ///
    /// `start` is the absolute frame position on the disc, including the 150-frame lead-in
    /// (as provided by [`TrackEntry::start`][crate::toc::TrackEntry::start]).
    ///
    /// Returns number of frames read. Errors with [`DeviceError::Unsupported`] if `buf` is not
    /// sized for an exact number of frames.
    fn read_cdda(&self, start: Frame, buf: &mut [u8]) -> Result<Frame, DeviceError>;

    /// Read the Table of Contents from the disc and provide a parsed representation as a
    /// platform-independent [TocEntries]. If a cached version is available then it may be cloned
    /// and returned rather than re-reading from disc.
    fn read_toc(&self) -> Result<TocEntries, Error>;

    /// Read the Table of Contents from the disc and provide a raw_bytes representation of the
    /// returned data. The exact format of this data may be platform specific.
    ///
    /// Unless you are providing low-level or debug functionality, you probably want to use
    /// [read_toc][Device::read_toc] instead which will provide a platform-independent [TocEntries]
    fn raw_toc(&self) -> Result<Vec<u8>, DeviceError>;
}

/// Implementors of `Drive` can additionally read an entire track and may hold a [`Context<RipProgress>`]
/// to allow for mid-rip termination and status updates.
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
    fn open<P: AsRef<Path> + ?Sized>(path: &P, cx: Context<RipProgress>) -> Result<Self, Error>;

    /// Retrieve the thread [Context].
    fn cx(&self) -> Context<RipProgress>;

    /// Read an entire track of data allowing cancellation mid-read and providing status updates
    /// via the thread [Context].
    ///
    /// # Errors
    /// - [`DriveError::NotFound`] if `track_number` does not identify an audio track on the disc
    /// - [`DriveError::UnexpectedEof`] if the track has zero duration
    /// - [`DriveError::FileTooLarge`] if the track is too long to buffer in memory
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

        let track_size = track.duration.size();

        (track_size > ByteSize::ZERO)
            .ok_or(DriveError::ZeroLengthTrack)
            .or_error("zero length track")?;

        // Vec needs to be initialised to split into chunks. Performance cost insignificant vs IO.
        let mut data = vec![0_u8; track_size.cast::<usize>()];

        let (bufs, last_buf) = data.as_chunks_mut::<{ MAX_CHUNK_BYTES.strict_cast() }>();
        let mut frames_read_so_far = Frame::default();
        trace_span.record("frames_read", format_args!("{frames_read_so_far}"));

        for (i, buf) in bufs.iter_mut().enumerate() {
            self.cx().cancelled()?;
            self.cx()
                .reply(RipProgress {
                    track_number,
                    processed: frames_read_so_far,
                    total: track.duration,
                })
                .or_error("sending progress update")?;

            // TODO #203 Simplify read_track by relying on frames_read_so_far and validate MAX_CHUNK_FRAMEs were read instead
            let offset = MAX_CHUNK_FRAMES
                .checked_mul(i)
                .ok_or(DriveError::InvalidData)
                .or_error("computing chunk offset")?;

            debug_assert_eq!(
                frames_read_so_far, offset,
                "now reading chunk {i} but have only read {frames_read_so_far} frames so far"
            );

            let start = track
                .toc_entry
                .start()
                .checked_add(offset)
                .ok_or(DriveError::InvalidData)
                .or_error("computing read start")?;

            let frames_read = self.device().read_cdda(start, buf)?;
            frames_read_so_far = frames_read_so_far
                .checked_add(frames_read)
                .ok_or(DriveError::InvalidData)
                .or_error("accumulating frames read")?;
            trace_span.record("frames_read", format_args!("{frames_read_so_far}"));
        }

        let offset = MAX_CHUNK_FRAMES
            .checked_mul(bufs.len())
            .ok_or(DriveError::InvalidData)
            .or_error("computing last chunk offset")?;
        debug_assert_eq!(
            offset, frames_read_so_far,
            "about to read last chunk. We are about to read from {offset}, but have only read {frames_read_so_far} frames so far"
        );

        if !last_buf.is_empty() {
            self.cx().cancelled()?;
            self.cx()
                .reply(RipProgress {
                    track_number,
                    processed: frames_read_so_far,
                    total: track.duration,
                })
                .or_error("sending progress update")?;

            let start = track
                .toc_entry
                .start()
                .checked_add(offset)
                .ok_or(DriveError::InvalidData)
                .or_error("computing last read start")?;

            let frames_read = self.device().read_cdda(start, last_buf)?;
            frames_read_so_far = frames_read_so_far
                .checked_add(frames_read)
                .ok_or(DriveError::InvalidData)
                .or_error("accumulating frames read")?;
            trace_span.record("frames_read", format_args!("{frames_read_so_far}"));
        }
        self.cx()
            .reply(RipProgress {
                track_number,
                processed: frames_read_so_far,
                total: track.duration,
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

    fn open<P: AsRef<Path> + ?Sized>(path: &P, cx: Context<RipProgress>) -> Result<Self, Error> {
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

    use crate::{Drive, test_fixtures::albums::TestAlbum::*, win::DriveHandle};

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
