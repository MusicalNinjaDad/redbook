#![expect(missing_docs, reason = "WIP")]
//! Provides a logical representation of an audio CD on the windows platform.

use std::{
    io::{self, ErrorKind},
    path::Path,
};

use cdtoc::Toc;
use thread_safely::Context;
use tracing_result::Trace;

use crate::{CdDrive, Disc, Drive, RippedTrack, TocEntries};

#[derive(Debug)]
pub struct AudioCd {
    drive: CdDrive,
    disc: Disc,
}

impl TryFrom<CdDrive> for AudioCd {
    type Error = io::Error;

    fn try_from(drive: CdDrive) -> Result<Self, Self::Error> {
        let _err_span =
            tracing::error_span!("AudioCd::TryFrom<CdDrive>", path = %drive.path().display())
                .entered();

        let toc = drive.toc();
        let tracks: Vec<_> = toc.audio_tracks().collect();
        let toc = Toc::try_from(toc).or_error("could not parse TOC in cdtoc format")?;

        let disc = Disc::new(toc, tracks)?;

        Ok(Self { drive, disc })
    }
}

impl AudioCd {
    pub fn open<P: AsRef<Path>>(path: &P, cx: Context<RipProgress>) -> io::Result<Self> {
        let drive = CdDrive::open(path, cx)?;
        Self::try_from(drive)
    }

    pub fn rip(&self, track_number: usize) -> io::Result<RippedTrack> {
        let _error = tracing::error_span!("AudioCdExt::rip", track_number).entered();
        let tags = self
            .disc
            .tag_for(track_number)
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "track not found"))
            .or_warn("unable to construct tags")?;
        let coverart = self.disc.cover_art().cloned();
        let raw_data = self.drive.read_track(track_number)?;
        Ok(RippedTrack {
            tags,
            coverart,
            raw_data,
        })
    }

    pub fn disc(&self) -> &Disc {
        &self.disc
    }

    pub fn disc_mut(&mut self) -> &mut Disc {
        &mut self.disc
    }

    pub fn path(&self) -> &Path {
        self.drive.path()
    }

    pub fn toc(&self) -> &Toc {
        self.disc.toc()
    }

    pub fn toc_entries(&self) -> &TocEntries {
        self.drive.toc()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A status update which is provided in the `reply` channel of a [thread_safely::Context] during
/// [rip][AudioCdExt::rip] and [read_track][AudioCdExt::read_track]
///
/// TODO: #114 Include device identifier in RipProgress
#[expect(missing_docs, reason = "well named fields")]
pub struct RipProgress {
    pub track_number: usize,
    pub bytes_processed: usize,
    pub total_bytes: usize,
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
        let cd = AudioCd::open(&path, Default::default()).unwrap();
        assert_eq!(album, TestAlbum::try_from(&cd).unwrap());
        assert_eq!(
            cd.disc.tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
        let toc_entries: Vec<_> = cd.disc.tracks().map(|track| track.toc_entry).collect();
        assert_eq!(album.expected_audio_toc_entries(), toc_entries);
    }
}
