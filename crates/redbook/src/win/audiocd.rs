//! Provides a logical representation of an audio CD on the windows platform.

use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

use thread_safely::Context;
use tracing_result::Trace;

use super::{drive::CdDrive, toc::CdaFile};
use crate::{AudioCdExt, Disc, Frame, RipProgress, TocEntry, Track};

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

impl AudioCd {
    /// Opens drive, reads CD
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let _err_span =
            tracing::error_span!("AudioCd::new", path = %path.as_ref().display()).entered();
        let drive = CdDrive::open(path)?;
        Self::try_from(drive)
    }

    /// Opens drive, reads CD, and stores a thread [Context][thread_safely::Context] to allow for
    /// cancellation and status updates during long-running reads.
    pub fn with_context<P: AsRef<Path>>(path: P, cx: Context<RipProgress>) -> io::Result<Self> {
        let mut cd = Self::new(path)?;
        cd.add_context(cx);
        Ok(cd)
    }

    /// Add a thread [Context] to an existing `AudioCd`
    pub fn add_context(&mut self, cx: Context<RipProgress>) {
        self.thread_context = cx;
    }
}

/// This will use a default thread [Context].
///
/// To store a context first create the `AudioCd`, then add it:
/// ```no_run
/// # use redbook::{RipProgress, win::{AudioCd, drive::CdDrive}};
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
        tracks.sort_by_key(|track| track.toc_entry.start);

        let wintoc = drive.toc();
        (wintoc.FirstTrack
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
        (wintoc.LastTrack
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

        for track in tracks.iter() {
            let track_number = track.toc_entry.track as usize;

            let _warn = tracing::warn_span!("validating TOC vs `.cda`s", track_number);

            let data = wintoc
                .TrackData
                .get(track_number - 1)
                .ok_or(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("track {track_number} missing in TOC"),
                ))
                .or_warn("")?;
            (TocEntry::from(data) == track.toc_entry)
                .ok_or(io::Error::new(
                    ErrorKind::InvalidData,
                    format!("Mismatch between TOC and cda files for track {track_number}"),
                ))
                .or_warn("")?;
        }

        let toc = wintoc
            .as_toc()
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
            .or_error("")?;

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

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    fn new(#[case] album: TestAlbum) {
        let path = album.assets_path();
        let cd = AudioCd::new(path).unwrap();
        #[expect(unsafe_code)]
        // SAFETY: not a real handle and no threads involved
        let handle = unsafe { cd.drive.handle() };
        assert_eq!(album, TestAlbum::try_from(*handle).unwrap());
        let toc_entries: Vec<_> = cd.disc().tracks().map(|track| track.toc_entry).collect();
        assert_eq!(album.expected_toc_entries(), toc_entries);
    }
}
