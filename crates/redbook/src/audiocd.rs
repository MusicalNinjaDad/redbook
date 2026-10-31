#![expect(missing_docs, reason = "WIP")]
//! Provides [`AudioCd`] & associated [`RipProgress`]

use std::{
    io::{self, ErrorKind},
    path::Path,
};

use cdtoc::Toc;
use thread_safely::Context;
use tracing_result::Trace;

use crate::{CdDrive, Disc, Drive, Frame, RippedTrack, TocEntries};

#[derive(Debug)]
/// A CD loaded into a physical drive. This combines the ability to access the physical disc with
/// additional data about the contents which is not stored on the disc itself.
///
/// This is the main entry point for the crate and the struct which binaries will most likely interact
/// with. It makes use of the platform-specific [`CdDrive`] and will always be well-behaved for the
/// target OS.
///
/// # Construction
///
/// ## Direct construction
///
/// **Direct construction** is possible from a path which represents the file-system entry for the
/// *device*. On windows, this is either:
///
/// **For CDs with no data sessions** ("normal" CDs)
/// - the drive letter (`E:`)
///
/// **For CDs with audio & data sessions** ("enhanced CDs")
/// - the device-type path (e.g. `CDROM0`), for any CD
/// - the full device path (e.g. `\\?\usbstor#cdrom&ven_hl-dt-st&prod_dvdram_gue1n&rev_as00#4b4d555642414e313035a920&0#{53f56308-b7bf-11d0-95f2-00a0e91efb8b}`
///
/// ```rust no_run
/// # use std::io;
/// # use redbook::AudioCd;
/// let mut cd = AudioCd::open("E:", Default::default())?;
/// # io::Result::Ok(())
/// ```
///
/// ## Indirect construction
///
/// **Indirect construction** is possible for cases where you do not know the correct path to provide.
/// [`CdDrives`][crate::CdDrives] provides an iterator over all available drives, and `AudioCd` implements
/// `TryFrom<CdDrive>`.
///
/// ```
/// # use std::io;
/// # use redbook::{AudioCd, CdDrives};
/// let first_drive = CdDrives::all(Default::default())?
///     .next()
///     .ok_or(io::Error::other("no drives found"))?;
/// let mut cd = AudioCd::try_from(first_drive)?;
/// # io::Result::Ok(())
/// ```
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
    pub fn open<P: AsRef<Path> + ?Sized>(path: &P, cx: Context<RipProgress>) -> io::Result<Self> {
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
/// A status update which is provided in the `reply` channel of a [`thread_safely::Context`] during
/// calls to [`rip`][AudioCd::rip] and [`read_track`][Drive::read_track]
///
/// TODO: #114 Include device identifier in RipProgress
#[expect(missing_docs, reason = "well named fields")]
pub struct RipProgress {
    pub track_number: usize,
    pub processed: Frame,
    pub total: Frame,
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
