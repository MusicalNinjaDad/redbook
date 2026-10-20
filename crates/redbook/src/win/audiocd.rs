//! Provides a logical representation of an audio CD on the windows platform.

use std::{io, path::Path};

use cdtoc::Toc;
use thread_safely::Context;
use tracing_result::Trace;

use crate::{AudioCdExt, CdDrive, Device, Disc, Drive, RipProgress, Track, win::sys::DriveHandle};

/// An AudioCd with potentially mutable metadata.
///
/// # Thread safety
///
/// This is !Send to avoid metadata diverging between threads.
///
/// Calls to [disc().clone()][AudioCdExt::disc] should be treated as having created a true
/// clone of the metadata as any future mutation will use [Arc::make_mut].
///
/// It is recommended that you call [lock()][Self::lock] to obtain a [ReadOnlyAudioCd] before
/// spawning any threads.
#[derive(Debug)]
pub struct AudioCd {
    drive: CdDrive<DriveHandle>,
    disc: Disc,
    thread_context: Context<RipProgress>,
}

/// This will use a default thread [Context].
///
/// To store a context first create the `AudioCd`, then add it:
/// ```no_run
/// # use redbook::{AudioCdExt, CdDrive, Device, RipProgress, win::{AudioCd, DriveHandle}};
/// # let drive: CdDrive<DriveHandle> = CdDrive::open("")?;
/// # let cx: thread_safely::Context<RipProgress> = Default::default();
/// let mut cd = AudioCd::try_from(drive)?;
/// cd.add_context(cx);
/// # std::io::Result::Ok(())
/// ```
impl TryFrom<CdDrive<DriveHandle>> for AudioCd {
    type Error = io::Error;

    fn try_from(drive: CdDrive<DriveHandle>) -> Result<Self, Self::Error> {
        let _err_span =
            tracing::error_span!("AudioCd::TryFrom<CdDrive>", path = %drive.path().display())
                .entered();

        let toc = drive.toc();
        let tracks: Vec<_> = toc.audio_tracks().collect();
        let toc = Toc::try_from(toc).or_error("")?;

        let disc = Disc::new(toc, tracks)?;

        Ok(Self {
            drive,
            disc,
            thread_context: Default::default(),
        })
    }
}

impl AudioCdExt for AudioCd {
    fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let _err_span =
            tracing::error_span!("AudioCd::new", path = %path.as_ref().display()).entered();
        let drive = CdDrive::open(path)?;
        Self::try_from(drive)
    }

    fn with_context<P: AsRef<Path>>(path: P, cx: Context<RipProgress>) -> io::Result<Self> {
        let mut cd = Self::new(path)?;
        cd.add_context(cx);
        Ok(cd)
    }

    fn add_context(&mut self, cx: Context<RipProgress>) {
        self.thread_context = cx;
    }

    fn cx(&self) -> Context<RipProgress> {
        self.thread_context.clone()
    }

    fn disc(&self) -> &Disc {
        &self.disc
    }

    fn disc_mut(&mut self) -> &mut Disc {
        &mut self.disc
    }

    fn read_chunk(
        &self,
        track: &Track,
        frame_offset: usize,
        frames_to_read: u32,
        buf: &mut [u8],
    ) -> io::Result<u32> {
        todo!()
    }
}

#[cfg(test)]
mod miri {
    use rstest::rstest;

    use crate::test_fixtures::albums::TestAlbum::{self, *};

    use super::*;

    impl TryFrom<&AudioCd> for TestAlbum {
        type Error = io::Error;

        fn try_from(audio_cd: &AudioCd) -> Result<Self, Self::Error> {
            let drive = &audio_cd.drive;
            drive.try_into()
        }
    }

    #[test_log::test]
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn new(#[case] album: TestAlbum) {
        let path = album.assets_path();
        let cd = AudioCd::new(path).unwrap();
        assert_eq!(album, TestAlbum::try_from(&cd).unwrap());
        assert_eq!(
            cd.disc().tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
        let toc_entries: Vec<_> = cd.disc().tracks().map(|track| track.toc_entry).collect();
        assert_eq!(album.expected_audio_toc_entries(), toc_entries);
    }
}
