//! Provides a logical representation of an audio CD on the windows platform.

use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
    sync::Arc,
};

use tracing_result::Trace;

use super::{drive::CdDrive, toc::CdaFile};
use crate::{AudioCdExt, AudioCdExtMut, Disc, Frame, TocEntry, Track};

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
#[derive(Debug, PartialEq)]
pub struct AudioCd {
    drive: CdDrive,
    disc: Arc<Disc>,
}

impl !Send for AudioCd {}

/// An AudioCd where metadata can no longer be mutated - ensuring safe Sync usage of
/// [disc()][AudioCdExt::disc] and allowing, for example, separate threads to [rip][AudioCdExt::rip]
/// and encode [to_flac][crate::RippedTrack::to_flac]
///
/// # Thread safety
///
/// `ReadOnlyAudioCd` is [`Send`] as an entire struct but not [`Sync`] as the underlying mechanisms
/// to read data from the Cd are not synchronised.
///
/// [`Sync`] references to the metadata can be obtained via [`disc().clone()`][AudioCdExt::disc]
/// and safely passed to other threads.
pub struct ReadOnlyAudioCd {
    drive: CdDrive,
    disc: Arc<Disc>,
}

impl AudioCd {
    /// Opens drive, reads CD
    pub fn new<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path_str = path.as_ref().display().to_string();

        const _TARGET: &str = "AudioCd::new";
        let _err_span = tracing::error_span!(_TARGET, path = %path_str).entered();

        // Windows already helpfully decodes the TOC for us. Parsing .cda files pre-calculates the
        // durations and gives us a comparison to validate the raw TOC against.
        let mut tracks: Vec<_> = fs::read_dir(&path)
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

        let drive = CdDrive::open(path)?;

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

        let disc = Arc::new(Disc::new(toc, tracks, Frame::new(leadout as usize))?);

        Ok(Self { drive, disc })
    }
}

impl AudioCdExt for AudioCd {
    fn disc(&self) -> &Arc<crate::Disc> {
        &self.disc
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

impl AudioCdExt for ReadOnlyAudioCd {
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

    fn disc(&self) -> &Arc<crate::Disc> {
        &self.disc
    }
}

impl AudioCdExtMut for AudioCd {
    fn disc_mut(&mut self) -> &mut Disc {
        Arc::make_mut(&mut self.disc)
    }

    #[expect(refining_impl_trait)]
    fn lock(self) -> ReadOnlyAudioCd {
        tracing::trace!(audiocd = ?self, "locking");

        ReadOnlyAudioCd {
            drive: self.drive,
            disc: self.disc,
        }
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use crate::test_fixtures::albums::TestAlbum::{self, *};

    use super::*;

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[should_panic(expected = "dm")]
    fn new(#[case] album: TestAlbum) {
        let path = album.assets_path();
        let cd = AudioCd::new(path).unwrap();
        #[expect(unsafe_code)]
        // SAFETY: not a real handle
        let handle = unsafe { cd.drive.handle() };
        assert_eq!(album, TestAlbum::try_from(*handle).unwrap());
    }
}
