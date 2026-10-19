//! Provides a logical representation of an audio CD on the windows platform.

use std::{
    io::{self, ErrorKind},
    iter::zip,
    path::Path,
};

use cdtoc::Toc;
use thread_safely::Context;
use tracing_result::Trace;

use super::{cda::CdaFile, drive::CdDrive};
use crate::{AudioCdExt, Disc, Frame, RipProgress, Track};

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
    drive: CdDrive,
    disc: Disc,
    thread_context: Context<RipProgress>,
}

/// This will use a default thread [Context].
///
/// To store a context first create the `AudioCd`, then add it:
/// ```no_run
/// # use redbook::{AudioCdExt, RipProgress, win::{AudioCd, drive::CdDrive}};
/// # let drive: CdDrive = CdDrive::open("")?;
/// # let cx: thread_safely::Context<RipProgress> = Default::default();
/// let mut cd = AudioCd::try_from(drive)?;
/// cd.add_context(cx);
/// # std::io::Result::Ok(())
/// ```
impl TryFrom<CdDrive> for AudioCd {
    type Error = io::Error;

    fn try_from(drive: CdDrive) -> Result<Self, Self::Error> {
        let _err_span =
            tracing::error_span!("AudioCd::TryFrom<CdDrive>", path = %drive.path().display())
                .entered();

        // Windows already helpfully decodes the TOC for us. Parsing .cda files pre-calculates the
        // durations and gives us a comparison to validate the raw TOC against.
        //
        // glob yields paths in alphabetical order
        let cda_tracks: Vec<_> = glob::glob(
            drive
                .path()
                .join("*.cda")
                .to_str()
                .ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "drive path not valid"))
                .or_error("unable to glob cda files")?,
        )
        .map_err(|err| io::Error::new(ErrorKind::InvalidFilename, err))
        .or_error("unable to glob cda files")?
        .filter_map(|cda| {
            cda.map_err(|error| io::Error::new(ErrorKind::InvalidFilename, error))
                .or_error("getting cda file")
                .ok()
        })
        .filter_map(|path| CdaFile::from_path(path).or_warn("invalid cda file").ok())
        .map(Track::from)
        .collect();

        let toc = drive.toc();

        if !cda_tracks.is_empty() {
            (cda_tracks.len() == toc.audio.len())
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "disc TOC reports {d} tracks but {c} cda files found",
                            d = toc.audio.len(),
                            c = cda_tracks.len()
                        ),
                    )
                })
                .or_error("comparing number of tracks in TOC vs cda files")?;

            for (t, (cda_track, toc_entry)) in zip(cda_tracks, toc).enumerate() {
                let track_number = t + 1;
                let _warn = tracing::warn_span!("validating TOC vs `.cda`s", track_number);

                ((toc_entry.track == cda_track.track_number())
                    && (toc_entry.track.strict_cast::<usize>() == track_number))
                    .ok_or_else(|| {
                        io::Error::new(ErrorKind::InvalidData, "track numbers are not sequential")
                    })?;

                (toc_entry == &cda_track.toc_entry)
                    .ok_or(io::Error::new(
                        ErrorKind::InvalidData,
                        format!("Mismatch between TOC and cda files for track {t}"),
                    ))
                    .or_warn("")?;
            }
        }

        let tracks: Vec<_> = toc.audio_tracks().collect();
        let toc = Toc::try_from(toc).or_error("")?;

        // TODO: Why do we need a leadout on Disc & document that it is the final one, if still needed
        let leadout = toc.leadout();

        let disc = Disc::new(toc, tracks, Frame::new(leadout as usize))?;

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
        self.drive
            .read_chunk(track, frame_offset, frames_to_read, buf)
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
