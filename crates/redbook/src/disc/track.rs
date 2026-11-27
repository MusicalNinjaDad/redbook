#![expect(missing_docs, reason = "TODO")]

use std::fmt::Display;

use crate::{
    Frame,
    disc::error::InvalidToc,
    toc::{Msf, TrackNo, tocentry::TrackEntry},
};

#[derive(Debug, Copy, Clone, PartialEq)]
/// A track on a CD with associated metadata.
///
/// This struct represents a single track on an audio CD, combining basic
/// TOC (Table of Contents) information with optional metadata from MusicBrainz.
///
/// # Notes
///
/// - Typically constructed as `<'static>` and then cloned when referencing metadata
pub struct Track<'meta> {
    trackno: TrackNo,
    start: Frame,
    /// Duration of this track in frames.
    duration: Frame,
    /// Optional reference to MusicBrainz track metadata.
    pub(crate) meta: Option<&'meta musicbrainz_rs::entity::release::Track>,
}

impl<'meta> Display for Track<'meta> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "track: {trackno}, start: {start}, duration: {dur}, has metadata: {meta}",
            trackno = self.trackno,
            start = self.start,
            dur = self.duration,
            meta = self.meta.is_some()
        )
    }
}

impl Track<'static> {
    /// Create a new `Track` with no MusicBrainz metadata
    ///
    /// # Notes
    /// - `track_number` needs no validation: a [`TrackNo`] is always a valid track number
    ///   (1..=99). "Special" POINT values (e.g. 0xA0..=0xA2, 0xAA) are
    ///   [`Point`][crate::toc::Point]s, not track numbers
    pub fn new(trackno: TrackNo, start: Frame, duration: Frame) -> Self {
        Self {
            trackno,
            start,
            duration,
            meta: None,
        }
    }
}

impl<'meta> Track<'meta> {
    /// Returns the 1-indexed track number.
    pub fn trackno(&self) -> TrackNo {
        self.trackno
    }

    pub fn start(&self) -> Frame {
        self.start
    }

    pub fn duration(&self) -> Frame {
        self.duration
    }

    /// Returns the track title from MusicBrainz metadata, if available.
    pub fn title(&self) -> Option<String> {
        self.meta.map(|track| track.title.clone())
    }

    /// Returns the most likely representation of the track listing, as we expect it was
    /// written on the back of the CD. The only adjustments we make are to ensure that numerical
    /// track numbers are always 2 digits long, in order to allow alphabetical sorting to work.
    ///
    /// # Notes
    ///
    /// - Uses the text representation for track number from MusicBrainz if available
    /// - Falls back to the two-digit track number if MusicBrainz data is not available
    /// - Numerical track numbers are always formatted as 2 digits (e.g., "05")
    pub fn filename(&self) -> String {
        let track_num = self
            .meta()
            .map(|trk| {
                let trk_num = trk.number.clone();
                match trk_num.len() {
                    1 if trk_num.parse::<usize>().is_ok() => format!("0{trk_num}"),
                    _ => trk_num,
                }
            })
            .unwrap_or_else(|| format!("{:02}", self.trackno));
        [track_num, self.title().unwrap_or_default()].join(" ")
    }

    /// Returns a reference to the MusicBrainz track metadata, if available.
    pub fn meta(&self) -> Option<&'meta musicbrainz_rs::entity::release::Track> {
        self.meta
    }
}

impl PartialEq<TrackEntry> for Track<'_> {
    fn eq(&self, track_entry: &TrackEntry) -> bool {
        track_entry.is_audio_track()
            && (self.trackno == track_entry.trackno())
            && (self.start == track_entry.start())
    }
}

impl PartialEq<cdtoc::Track> for Track<'_> {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (self.trackno.cast::<u8>() == toc_track.number())
            && (Msf::try_from(toc_track.msf()).is_ok_and(|msf| self.start == msf))
            && (self.duration == toc_track.duration())
    }
}

impl TryFrom<cdtoc::Track> for Track<'static> {
    type Error = InvalidToc;

    fn try_from(cdtoc_track: cdtoc::Track) -> Result<Self, Self::Error> {
        let trackno = TrackNo::new(cdtoc_track.number())?;
        let start = Msf::try_from(cdtoc_track.msf())
            .map_err(InvalidToc::InvalidStart)?
            .into();
        let duration =
            Frame::try_from(cdtoc_track.duration()).map_err(InvalidToc::InvalidDuration)?;
        Ok(Self {
            trackno,
            start,
            duration,
            meta: None,
        })
    }
}
