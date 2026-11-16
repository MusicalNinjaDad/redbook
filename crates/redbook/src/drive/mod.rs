//! Physical drives which can read CDs.

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use thread_safely::Context;

pub use error::DriveError;

use crate::{Error, Frame, RipProgress, TocEntries, frame::ByteSize};

#[doc(inline)]
pub use device::Device;
#[doc(inline)]
pub use drive::Drive;

mod device;
mod drive;
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
    use rstest::rstest;

    use crate::{
        Drive, TrackNo,
        disc::Track,
        sys::win::DriveHandle,
        test_fixtures::albums::TestAlbum::{self, *},
    };

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

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn read_track2(#[case] album: TestAlbum) {
        let drive = CdDrive::<DriveHandle>::open(&album.assets_path(), Default::default()).unwrap();
        let track2 = drive.read_track(TrackNo::strict_new(2)).unwrap();

        let (frames, rest) = track2.as_chunks::<{ ByteSize::ONE_FRAME.cast::<usize>() }>();

        assert_eq!(rest.len(), 0);
        assert_eq!(
            frames
                .first()
                .map(|bytes| u32::from_le_bytes(bytes[..4].try_into().unwrap()))
                .map(Frame::strict_new),
            album
                .expected_audio_tracks_minimal()
                .get(1)
                .map(Track::start)
        );

        assert_eq!(
            frames
                .last()
                .map(|bytes| u32::from_le_bytes(bytes[..4].try_into().unwrap()))
                .map(Frame::strict_new),
            album.expected_audio_tracks_minimal().get(1).map(|trk| trk
                .start()
                .strict_add(trk.duration())
                .strict_sub(Frame::strict_new(1)))
        );
    }
}
