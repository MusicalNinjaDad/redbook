//! Provides a logical representation of an audio CD on the windows platform.

use std::{
    fs,
    io::{self, ErrorKind},
    iter::zip,
    path::Path,
};

use cdtoc::Toc;
use thread_safely::Context;
use tracing_result::Trace;

use super::{drive::CdDrive, toc::CdaFile};
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
        let mut tracks: Vec<_> = fs::read_dir(drive.path())
            .or_error("open drive as dir")?
            .filter_map(|track| {
                let path =
                    try bikeshed io::Result<_> { track.or_error("read dir entry for cda")?.path() }
                        .ok()?;
                (path.extension()? == "cda").then(|| try bikeshed io::Result<_> {
                    let cda = CdaFile::from_path(path).or_error("read cda")?;
                    Track::from(cda)
                })
            })
            .try_collect()
            .or_error("parse cda")?;

        let wintoc = drive.toc();

        if !tracks.is_empty() {
            tracks.sort_by_key(|track| track.toc_entry.start);
            (wintoc
                .audio
                .first()
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no audio tracks"))
                .or_error("not an audio CD")?
                .track
                == tracks
                    .first()
                    .ok_or(io::Error::new(ErrorKind::NotFound, "no .cda files found"))
                    .or_warn("identifying first track")?
                    .toc_entry
                    .track as u8)
                .ok_or(io::Error::new(
                    ErrorKind::InvalidData,
                    "Mismatch between TOC and cda files: different first track number",
                ))
                .or_warn("identifying first track")?;
            (wintoc
                .audio
                .last()
                .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no audio tracks"))
                .or_error("not an audio CD")?
                .track
                == tracks
                    .last()
                    .ok_or(io::Error::new(ErrorKind::NotFound, "no .cda files found"))
                    .or_warn("identifying last track")?
                    .toc_entry
                    .track as u8)
                .ok_or(io::Error::new(
                    ErrorKind::InvalidData,
                    "Mismatch between TOC and cda files: different last track number",
                ))
                .or_warn("identifying last track")?;

            (tracks.len() == wintoc.audio.len())
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::InvalidData,
                        format!(
                            "disc TOC reports {d} tracks but {c} cda files found",
                            d = wintoc.audio.len(),
                            c = tracks.len()
                        ),
                    )
                })
                .or_error("comparing number of tracks in TOC vs cda files")?;

            for (t, (track, wintoc_entry)) in zip(tracks.iter(), wintoc.audio.iter()).enumerate() {
                let track_number = t + 1;
                let _warn = tracing::warn_span!("validating TOC vs `.cda`s", track_number);

                ((wintoc_entry.track == track.track_number())
                    && (wintoc_entry.track.strict_cast::<usize>() == track_number))
                    .ok_or_else(|| {
                        io::Error::new(ErrorKind::InvalidData, "track numbers are not sequential")
                    })?;

                (wintoc_entry == &track.toc_entry)
                    .ok_or(io::Error::new(
                        ErrorKind::InvalidData,
                        format!("Mismatch between TOC and cda files for track {t}"),
                    ))
                    .or_warn("")?;
            }
        } else {
            let toc_tracks = wintoc
                .audio
                .iter()
                .map_windows(|[track, next]| {
                    track.is_audio_track().then(|| {
                        let mut duration = next.start - track.start;
                        if next.is_data_track() {
                            duration -= Frame::new(11400)
                        }
                        Track::new(track.track, track.start, duration)
                    })
                })
                .flatten();
            tracks.extend(toc_tracks);
        }

        let toc = Toc::try_from(wintoc).or_error("")?;

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
