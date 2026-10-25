//! Support for Tables of Contents & Frame-based time

use std::{
    collections::HashMap,
    fmt::Display,
    hint::cold_path,
    io::{self, ErrorKind},
    num::NonZeroUsize,
    slice,
};

use bitflags::bitflags;
use cdtoc::Toc;
use itertools::{EitherOrBoth, Itertools};

use crate::{Frame, Track, frame::Msf};

pub mod b0000 {
    /// 0b0010 format track identifier for Leadout
    pub const LEADOUT_TRACK_ID: u8 = 0xAA;
}

pub mod b0010 {
    pub const FIRST_TRACK_ID: u8 = 0xA0;
    pub const LAST_TRACK_ID: u8 = 0xA1;
    /// 0b0010 format track identifier for Leadout
    pub const LEADOUT_TRACK_ID: u8 = 0xA2;
}

/// Entry in a CD TOC (Table of Contents) for an audio track.
///
/// Represents a single entry from the CD's Table of Contents, containing the
/// track number and its absolute start position on the disc.
///
/// # Notes
///
/// - The start position includes the lead-in area (150 frames)
/// - As this library is focussed on audio (redbook), only data relevant to audio tracks is
///   maintained. Conversion functions should be designed discard or fail on data tracks.
///
/// # TODO
///
/// - Example
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct TocEntry {
    /// The 1-indexed track number.
    pub track: u8,
    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    ///
    /// TODO: #127 Store Msf in TocEntry (to match technical impl on discs & cover wierd A0 etc)
    pub start: Frame,
    /// The control code, identifying track type
    pub control: Control,
}

impl Display for TocEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "track: {n}, start: {start}, control codes: {ctl}",
            n = self.track,
            start = self.start,
            ctl = self.control
        )
    }
}

impl PartialEq<cdtoc::Track> for TocEntry {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (self.track == toc_track.number()) && (self.start == Msf::from(toc_track.msf()))
    }
}

/// Q Sub-Channel control flags, as per table 222 of MMC-3 draft (section 5.22.2)
///
/// # Interpretation
/// | Value |  | Definition |
/// |------:|--|:-----------|
/// | 00x0b |  | 2 audio channels without pre-emphasis |
/// | 00x1b |  | 2 audio channels with pre-emphasis of 50/15 µs |
/// | 10x0b |  | audio channels without pre-emphasis (reserved in CD-R/RW) |
/// | 10x1b |  | audio channels with pre-emphasis of 50/15 µs (reserved in CD-R/RW) |
/// | 01x0b |  | Data track, recorded uninterrupted |
/// | 01x1b |  | Data track, recorded incremental |
/// | 11xxb |  | Reserved |
/// | xx0xb |  | digital copy prohibited |
/// | xx1xb |  | digital copy permitted |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Control(u8);

bitflags! {
        impl Control: u8 {
        /// pre-emphasis of 50/15 µs
        const PRE_EMPHASIS = 0b0001;
        /// digital copy-permitted
        const COPY_PERMITTED = 0b0010;
        /// data track
        const DATA_TRACK = 0b0100;
        /// non-stereo audio
        const NON_STEREO_AUDIO = 0b1000;
    }
}

impl Control {
    /// represents a data track
    pub fn is_data(&self) -> bool {
        self.contains(Control::DATA_TRACK)
    }

    /// represents an audio track
    pub fn is_audio(&self) -> bool {
        !self.is_data()
    }
}

impl Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        bitflags::parser::to_writer_strict(self, f)
    }
}

impl TocEntry {
    /// Is an audio track (based on control code & track number)
    pub fn is_audio_track(&self) -> bool {
        self.control.is_audio() && self.track <= 0x63
    }

    /// Is an data track (based on control code & track number)
    pub fn is_data_track(&self) -> bool {
        // TODO: #122 Tracknumbers: types & consts
        self.control.is_data() && self.track <= 0x63
    }

    /// Returns the kind of track (based on control code & track number)
    pub fn kind(&self) -> TocEntryKind {
        match (self.control.is_data(), self.track) {
            (true, ..=0x63) => TocEntryKind::Data,
            (false, ..=0x63) => TocEntryKind::Audio,
            (true, b0000::LEADOUT_TRACK_ID) | (true, b0010::LEADOUT_TRACK_ID) => {
                TocEntryKind::DataLeadout
            }
            (false, b0000::LEADOUT_TRACK_ID) | (false, b0010::LEADOUT_TRACK_ID) => {
                TocEntryKind::AudioLeadout
            }
            (true, b0010::FIRST_TRACK_ID) => TocEntryKind::FirstDataTrack,
            (false, b0010::FIRST_TRACK_ID) => TocEntryKind::FirstAudioTrack,
            (true, b0010::LAST_TRACK_ID) => TocEntryKind::LastDataTrack,
            (false, b0010::LAST_TRACK_ID) => TocEntryKind::LastAudioTrack,
            _ => TocEntryKind::Unknown,
        }
    }
}

/// The kind of TocEntry. Returned by [TocEntry::kind] and used in [TocEntries::validate_leadouts]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TocEntryKind {
    /// An audio track
    Audio,
    /// A data track
    Data,
    /// The leadout for an audio session
    AudioLeadout,
    /// The leadout for a data session
    DataLeadout,
    /// First track number, encoded as start minute
    FirstAudioTrack,
    /// Last track number, encoded as start minute
    LastAudioTrack,
    /// First track number, encoded as start minute
    FirstDataTrack,
    /// Last track number, encoded as start minute
    LastDataTrack,
    /// An entry type which we don't expect and don't understand
    Unknown,
}

impl TocEntryKind {
    pub fn is_info(&self) -> bool {
        !matches!(
            self,
            TocEntryKind::Audio
                | TocEntryKind::Data
                | TocEntryKind::AudioLeadout
                | TocEntryKind::DataLeadout
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A collection of [`TocEntry`]s, neatly grouped and sorted.
///
/// Construction is available via FromIterator<TocEntry> for io::Result<TocEntries>
pub struct TocEntries {
    entries: Vec<TocEntry>,
    track_types: HashMap<TocEntryKind, usize>,
    leadouts: Leadouts,
}

pub trait TocEntryFilter<'e>: Iterator<Item = &'e TocEntry> + DoubleEndedIterator + Sized {
    fn audio(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::Audio))
    }

    fn data(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::Data))
    }

    fn leadouts(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| {
            matches!(entry.kind(), TocEntryKind::AudioLeadout)
                || matches!(entry.kind(), TocEntryKind::DataLeadout)
        })
    }

    fn final_leadout(self) -> Option<&'e TocEntry> {
        self.rev().leadouts().next()
    }

    fn first_audio_info(self) -> Option<u8> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::FirstAudioTrack))
            .map(|entry| Msf::from(entry.start).min)
            .next()
    }

    fn last_audio_info(self) -> Option<u8> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::LastAudioTrack))
            .map(|entry| Msf::from(entry.start).min)
            .next()
    }

    fn first_data_info(self) -> Option<u8> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::FirstDataTrack))
            .map(|entry| Msf::from(entry.start).min)
            .next()
    }

    fn last_data_info(self) -> Option<u8> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::LastDataTrack))
            .map(|entry| Msf::from(entry.start).min)
            .next()
    }
}

impl<'e, I> TocEntryFilter<'e> for I where
    I: Iterator<Item = &'e TocEntry> + DoubleEndedIterator + Sized
{
}

impl TocEntries {
    /// Provides an iterator over *borrowed* [&TocEntry][TocEntry]s.
    ///
    /// # Note
    ///
    /// [TocEntries] does not directly implement [IntoIterator], but `&TocEntries` does.
    /// Either use `TocEntries.iter()` or `&TocEntries` in a for loop or anywhere else which
    /// expects an `IntoIterator`.
    pub fn iter(&self) -> slice::Iter<'_, TocEntry> {
        self.into_iter()
    }

    /// Provides an iterator over the audio tracks as [`Track`]s with duration calculated based
    /// upon the start position of the next track / leadout.
    ///
    /// # TODO
    /// - TODO: #110 handle discs with data before audio, or multiple audio sessions
    pub fn audio_tracks(&self) -> impl Iterator<Item = Track<'static>> {
        const OFFSET_BEFORE_DATA: Frame = Frame::new(11400);

        self.iter()
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then(|| {
                    let mut duration = next_track.start - this_track.start;
                    if next_track.is_data_track() {
                        duration -= OFFSET_BEFORE_DATA
                    }
                    Track::new(this_track.track, this_track.start, duration)
                })
            })
            .flatten()
    }

    /// Get a given *audio* track. Returns `None` if the track number is invalid or not an audio track.
    pub fn track(&self, track: usize) -> Option<Track<'static>> {
        const OFFSET_BEFORE_DATA: Frame = Frame::new(11400);

        self.iter()
            .skip_while(|entry| entry.track.strict_cast::<usize>() != track)
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then(|| {
                    let mut duration = next_track.start - this_track.start;
                    if next_track.is_data_track() {
                        duration -= OFFSET_BEFORE_DATA
                    }
                    Track::new(this_track.track, this_track.start, duration)
                })
            })
            .flatten()
            .next()
    }
}

/// Whether only the final leadout is present, or leadouts are available for every session.
///
/// # Interpretation
/// - `Session(1)`: Only one session is present
/// - `Final`: More than one session is present, intra-session leadout positions are unknown
/// - `Session(n)`: More than one session is present,  intra-session leadout positions are available
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Leadouts {
    /// Multiple sessions exist but only the final leadout is available
    Final,
    /// Every session has a leadout available, includes the number of sessions found.
    PerSession(NonZeroUsize),
}

impl<'entries> IntoIterator for &'entries TocEntries {
    type Item = &'entries TocEntry;

    type IntoIter = std::slice::Iter<'entries, TocEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl<'meta, V> PartialEq<V> for TocEntries
where
    for<'v> &'v V: IntoIterator<Item = &'v Track<'meta>>,
{
    fn eq(&self, tracks: &V) -> bool {
        for pair in self.iter().zip_longest(tracks) {
            match pair {
                EitherOrBoth::Both(entry, track) if entry == &track.toc_entry => {}
                _ => {
                    cold_path();
                    return false;
                }
            }
        }
        true
    }
}

impl TryFrom<TocEntries> for Toc {
    type Error = io::Error;

    fn try_from(entries: TocEntries) -> Result<Self, Self::Error> {
        Self::try_from(&entries)
    }
}

impl TryFrom<&TocEntries> for Toc {
    type Error = io::Error;

    fn try_from(entries: &TocEntries) -> Result<Self, Self::Error> {
        // TODO: Don't panic! (via #106)
        let audio = entries
            .iter()
            .audio()
            .map(|entry| entry.start.as_usize().strict_cast::<u32>())
            .collect();
        let data = entries
            .iter()
            .rev()
            .data()
            .next()
            .map(|entry| entry.start.as_usize().strict_cast::<u32>());
        let leadout = entries
            .iter()
            .final_leadout()
            .map(|entry| entry.start.as_usize().strict_cast::<u32>())
            .expect("Must have at least one leadout or construction would have failed");
        Toc::from_parts(audio, data, leadout)
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
    }
}

impl FromIterator<TocEntry> for io::Result<TocEntries> {
    fn from_iter<I: IntoIterator<Item = TocEntry>>(iter: I) -> Self {
        use Leadouts::*;
        use TocEntryKind::*;

        let (info, mut entries): (Vec<_>, Vec<_>) =
            iter.into_iter().partition(|entry| entry.kind().is_info());

        (!entries.is_empty())
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no TOC entries"))?;

        // INVARIANT: all fields are expected to be sorted by start position
        entries.sort_by_key(|entry| entry.start);

        entries
            .first()
            .is_some_and(|track| track.is_audio_track() || track.is_data_track())
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidData, "first track is a leadout"))?;

        if !info.is_empty() {
            let first_audio_info = info.iter().first_audio_info();
            let first_audio = entries.iter().audio().next().map(|entry| entry.track);
            (first_audio_info == first_audio).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims first audio track is number {first_audio_info:?}, but first audio track found is number {first_audio:?}")
            ))?;

            let last_audio_info = info.iter().last_audio_info();
            let last_audio = entries.iter().rev().audio().next().map(|entry| entry.track);
            (last_audio_info == last_audio).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims last audio track is number {last_audio_info:?}, but last audio track found is number {last_audio:?}")
            ))?;

            let first_data_info = info.iter().first_data_info();
            let first_data = entries.iter().data().next().map(|entry| entry.track);
            (first_audio_info == first_audio).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims first data track is number {first_data_info:?}, but first audio track found is number {first_data:?}")
            ))?;

            let last_data_info = info.iter().last_data_info();
            let last_data = entries.iter().rev().data().next().map(|entry| entry.track);
            (last_audio_info == last_audio).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims last data track is number {last_data_info:?}, but last audio track found is number {last_data:?}")
            ))?;
        }

        let track_types = entries.iter().map(|entry| entry.kind()).counts();

        let sessions = track_types
            .get(&TocEntryKind::Data)
            .copied()
            .unwrap_or_default()
            + 1;

        let leadout_count = track_types
            .get(&TocEntryKind::DataLeadout)
            .copied()
            .unwrap_or_default()
            + track_types
                .get(&TocEntryKind::AudioLeadout)
                .copied()
                .unwrap_or_default();

        let leadouts = match leadout_count {
            1 => Leadouts::Final,
            s if s == sessions => Leadouts::PerSession(
                NonZeroUsize::try_from(sessions)
                    .map_err(|_| io::Error::new(ErrorKind::InvalidData, "no tracks present"))?,
            ),
            _ => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "incorrect number of leadouts. Expected 1 or {sessions} found {leadout_count}"
                    ),
                ));
            }
        };

        entries.iter().map_windows(|[this_entry, next_entry]| (*this_entry, *next_entry))
        .try_for_each(|(this_entry, next_entry)| {
            match (this_entry.kind(), next_entry.kind(), leadouts) {
                (Audio, Audio, _) => Ok(()),
                (Audio, Data, Final) => Ok(()),
                (Audio, AudioLeadout, _) => Ok(()),
                (Data, Audio, Final) => Ok(()),
                (Data, Data, Final) => Ok(()),
                (Data, DataLeadout, _) => Ok(()),
                (AudioLeadout, _, PerSession(_)) => Ok(()),
                (DataLeadout, _, PerSession(_)) => Ok(()),
                _ => Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "invalid toc entry order. {this_entry} cannot be followed by {next_entry}"
                    ),
                )),
            }})?;

        let tocentries = TocEntries {
            entries,
            track_types,
            leadouts,
        };

        Ok(tocentries)
    }
}
