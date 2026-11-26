//! Physical drives which can read CDs.

use std::{
    fmt::Debug,
    path::{Path, PathBuf},
};

use thread_safely::Context;

pub use error::DriveError;
use tracing::instrument;

use crate::{Error, RipProgress, TocEntries, toc::ByteSize};

pub use device::Device;
pub use drive::Drive;

mod device;
mod drive;
pub mod error;

#[derive(Debug, Clone)]
/// TODO: Docs
pub struct CdDrive<DEV: Device> {
    path: PathBuf,
    device: DEV,
    tocentries: TocEntries,
    chunk_size: ByteSize,
    cx: Context<RipProgress>,
}

impl<DEV: Device> CdDrive<DEV> {
    /// Construct a new CdDrive from constituent parts. You will almost never want to call this directly
    /// unless implementing `From`/`TryFrom`. Use [`open`][Drive::open] instead.
    #[instrument(level = "error", skip(device, cx))]
    pub fn new(path: PathBuf, device: DEV, cx: Context<RipProgress>) -> Result<Self, DriveError> {
        let tocentries = device.read_toc()?;
        let chunk_size = device.raw_max_chunk().map(ByteSize::rounded_new)?;
        Ok(Self {
            path,
            device,
            tocentries,
            chunk_size,
            cx,
        })
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
        Self::new(path, device, cx)
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

    fn max_chunk(&self) -> ByteSize {
        self.chunk_size
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
            CdDrive::new(path, device, cx)?
        };

        Some(drive)
    }
}

#[cfg(test)]
mod miri {
    use cdtoc::Toc;
    use rstest::rstest;

    use crate::{
        Drive, Frame, TrackNo,
        disc::Track,
        sys::win::DriveHandle,
        test_fixtures::albums::TestAlbum::{self, *},
        toc::ByteSize,
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
    fn max_chunk(#[case] album: TestAlbum) {
        let drive = CdDrive::<DriveHandle>::open(&album.assets_path(), Default::default()).unwrap();
        let frame_based = drive.max_chunk();
        assert_eq!(frame_based, album.expected_max_chunk());

        let raw = drive.device().raw_max_chunk().unwrap();

        assert!(frame_based.cast::<usize>() <= raw);
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
