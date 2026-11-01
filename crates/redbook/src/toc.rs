#![expect(missing_docs, reason = "TODO docs")]
//! Support for Tables of Contents

use std::{
    collections::HashMap,
    fmt::Display,
    hint::cold_path,
    io::{self, ErrorKind},
    marker::StructuralPartialEq,
    num::{NonZeroUsize, ParseIntError},
    ops::{Deref, DerefMut},
    slice,
    str::FromStr,
};

use bitflags::bitflags;
use cdtoc::Toc;
use itertools::{EitherOrBoth, Itertools};

use crate::{
    Frame, Track,
    frame::{Msf, UpperBounded},
};

pub mod b0000 {
    use super::TrackNo;
    /// 0b0010 format track identifier for Leadout
    pub const LEADOUT_TRACK_ID: TrackNo = TrackNo(0xAA);
}

pub mod b0010 {
    use super::TrackNo;
    pub const FIRST_TRACK_ID: TrackNo = TrackNo(0xA0);
    pub const LAST_TRACK_ID: TrackNo = TrackNo(0xA1);
    /// 0b0010 format track identifier for Leadout
    pub const LEADOUT_TRACK_ID: TrackNo = TrackNo(0xA2);
}

/// A valid track number.
///
/// # Notes
/// - Track numbers are stored on physical discs in a single 1-byte
/// - This is "simply" a type-safe wrapper to portray semantics & provide a clear statement that
///   track numbers are inherently u8
/// - No additional invariants are upheld. A TrackNo may be a "normal" track (1 to 99 as per
///   redbook) or a "special" number as regularly (ab)used in the redbook spec.
///   e.g. 0xAA ([`b0000::LEADOUT_TRACK_ID`]) or 0xA2 ([`b0010::LEADOUT_TRACK_ID`]).
/// - [`is_normal()`][Self::is_normal] & [`is_special()`][Self::is_special] are available for
///   identification, while [`new_normal()`][Self::new_normal] can be used as a validating constrcutor
///   if needed.
/// - Basic construction is via `TrackNo(n)` for simplicity.
/// - As such `TrackNo` implements `Deref` & `DerefMut` with `Target = u8` and can be used
///   interchangably with a `u8`
///
/// # Converting to other integers
/// - Use the provided `track_no.cast()` which is nicer to read than `track_no.0.into()`
///
/// # Arithmetic operations
/// - `+`, `-`, etc. are deliberately not implemented for TrackNo. While you *could* simply do
///   `TrackNo(trackno.0 + 3)`, `Deref<Target=u8>` provides all the `strict_`, `checked_`,
///   `saturating_` and `wrapping_` operations from `u8`. You are strongly advised to use these.
///
/// # Equality & Ordering
/// - `PartialEq` & `PartialOrd` are deliberately not implemented between `TrackNo` & `u8` to ensure
///   comparisons don't inadvertantly compare a TrackNo with something semantically different
#[expect(clippy::derived_hash_with_manual_eq, reason = "manual const impl")]
#[derive(Debug, Clone, Copy, Hash, Default)]
pub struct TrackNo(pub u8);

impl TrackNo {
    /// The minimum "normal" track number: track 1
    ///
    /// Redbook defines the valid range for "normal" track numbers as 0..=99
    pub const MIN_NORMAL: Self = Self(1);

    /// The maximum "normal" track number: track 99
    ///
    /// Redbook defines the valid range for "normal" track numbers as 0..=99
    pub const MAX_NORMAL: Self = Self(99);

    /// Construct a "normal" [`TrackNo`] from any integer type.
    ///
    /// Returns `None` if the value either:
    /// - does not fit in a `u8` (track numbers are stored on disc in a single byte), or
    /// - is not a "normal" track number, i.e. outside
    ///   [MIN_NORMAL][Self::MIN_NORMAL]..=[MAX_NORMAL][Self::MAX_NORMAL]
    ///
    /// # Notes
    /// - Track 0 and values above 99 have various "special" meanings depending on context
    ///   (e.g. the lead-out track is 0xAA). Construct those directly, e.g. via
    ///   [`b0000::LEADOUT_TRACK_ID`]
    ///
    /// # Examples
    /// ```
    /// use redbook::TrackNo;
    ///
    /// assert_eq!(TrackNo::new_normal(7_u16), Some(TrackNo(7)));
    /// assert_eq!(TrackNo::new_normal(0_u8), None);
    /// assert_eq!(TrackNo::new_normal(100_u8), None);
    /// assert_eq!(TrackNo::new_normal(256_u16), None);
    /// ```
    pub fn new_normal<T>(value: T) -> Option<Self>
    where
        u8: TryFrom<T>,
    {
        let track_no = u8::try_from(value).ok()?;
        Some(Self(track_no)).filter(Self::is_normal)
    }

    /// Whether this is a "special" track number, i.e. outside
    /// [MIN_NORMAL][Self::MIN_NORMAL]..=[MAX_NORMAL][Self::MAX_NORMAL]
    ///
    /// Track 0 and > 99 have various "special" meanings depending on context
    pub const fn is_special(&self) -> bool {
        !self.is_normal()
    }

    /// Whether this is a "normal" track number, i.e. within
    /// [MIN_NORMAL][Self::MIN_NORMAL]..=[MAX_NORMAL][Self::MAX_NORMAL]
    ///
    /// Track 0 and > 99 have various "special" meanings depending on context
    pub const fn is_normal(&self) -> bool {
        self.0 >= Self::MIN_NORMAL.0 && self.0 <= Self::MAX_NORMAL.0
    }

    /// Cast to another integer type (except i8)
    pub const fn cast<N>(self) -> N
    where
        N: [const] From<u8>,
    {
        N::from(self.0)
    }
}

// ==========================================
// TrackNo - const comparison & conversion ops
// ==========================================

const impl Ord for TrackNo {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

const impl PartialOrd for TrackNo {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

const impl Eq for TrackNo {}

const impl PartialEq for TrackNo {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl StructuralPartialEq for TrackNo {}

impl Deref for TrackNo {
    type Target = u8;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TrackNo {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Display for TrackNo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "track: {n}", n = self.0)
    }
}

const impl From<u8> for TrackNo {
    fn from(n: u8) -> Self {
        TrackNo(n)
    }
}

const impl From<TrackNo> for u8 {
    fn from(track_no: TrackNo) -> Self {
        track_no.0
    }
}

impl FromStr for TrackNo {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.parse()?))
    }
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
    pub track: TrackNo,
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
            "{n}, start: {start}, control codes: {ctl}",
            n = self.track,
            start = self.start,
            ctl = self.control
        )
    }
}

impl PartialEq<cdtoc::Track> for TocEntry {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (self.track == TrackNo(toc_track.number()))
            && Msf::try_from(toc_track.msf()).is_ok_and(|msf| self.start == msf)
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
        self.control.is_audio() && self.track.is_normal()
    }

    /// Is an data track (based on control code & track number)
    pub fn is_data_track(&self) -> bool {
        // TODO: #122 Tracknumbers: types & consts
        self.control.is_data() && self.track.is_normal()
    }

    /// Returns the kind of track (based on control code & track number)
    pub fn kind(&self) -> TocEntryKind {
        match (self.control.is_data(), self.track) {
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
            (true, n) if n.is_normal() => TocEntryKind::Data,
            (false, n) if n.is_normal() => TocEntryKind::Audio,
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

    /// Parse the entry detailing the track number of the first audio track, if present
    fn first_audio_info(self) -> Option<TrackNo> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::FirstAudioTrack))
            .map(|entry| Msf::from(entry.start).min)
            .map(Into::into)
            .next()
    }

    /// Parse the entry detailing the track number of the first last track, if present
    fn last_audio_info(self) -> Option<TrackNo> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::LastAudioTrack))
            .map(|entry| Msf::from(entry.start).min)
            .map(Into::into)
            .next()
    }

    /// Parse the entry detailing the track number of the first data track, if present
    fn first_data_info(self) -> Option<TrackNo> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::FirstDataTrack))
            .map(|entry| Msf::from(entry.start).min)
            .map(Into::into)
            .next()
    }

    /// Parse the entry detailing the track number of the last data track, if present
    fn last_data_info(self) -> Option<TrackNo> {
        self.filter(|entry| matches!(entry.kind(), TocEntryKind::LastDataTrack))
            .map(|entry| Msf::from(entry.start).min)
            .map(Into::into)
            .next()
    }
}

impl<'e, I> TocEntryFilter<'e> for I where
    I: Iterator<Item = &'e TocEntry> + DoubleEndedIterator + Sized
{
}

impl TocEntries {
    // There is a fixed offset between audio & data sessions
    const OFFSET_BEFORE_DATA: Frame = Frame::strict_new(11400);

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
        self.iter()
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then(|| {
                    let mut duration = next_track.start.saturating_sub(this_track.start);
                    if next_track.is_data_track() {
                        duration = duration.saturating_sub(Self::OFFSET_BEFORE_DATA)
                    }
                    Track::new(this_track.track, this_track.start, duration)
                })
            })
            .flatten()
    }

    /// Get a given *audio* track. Returns `None` if the track number is invalid or not an audio track.
    pub fn track(&self, track: TrackNo) -> Option<Track<'static>> {
        self.iter()
            .skip_while(|entry| entry.track != track)
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then(|| {
                    let mut duration = next_track.start.saturating_sub(this_track.start);
                    if next_track.is_data_track() {
                        duration = duration.saturating_sub(Self::OFFSET_BEFORE_DATA)
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
            .map(|entry| entry.start.cast::<u32>())
            .collect();
        let data = entries
            .iter()
            .rev()
            .data()
            .next()
            .map(|entry| entry.start.cast::<u32>());
        let leadout = entries
            .iter()
            .final_leadout()
            .map(|entry| entry.start.cast::<u32>())
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
            let first_audio_found = entries.iter().audio().next().map(|entry| entry.track);
            (first_audio_info == first_audio_found).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims first audio track is number {first_audio_info:?}, but first audio track found is number {first_audio_found:?}")
            ))?;

            let last_audio_info = info.iter().last_audio_info();
            let last_audio_found = entries.iter().rev().audio().next().map(|entry| entry.track);
            (last_audio_info == last_audio_found).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims last audio track is number {last_audio_info:?}, but last audio track found is number {last_audio_found:?}")
            ))?;

            let first_data_info = info.iter().first_data_info();
            let first_data_found = entries.iter().data().next().map(|entry| entry.track);
            (first_data_info == first_data_found).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims first data track is number {first_data_info:?}, but first audio track found is number {first_data_found:?}")
            ))?;

            let last_data_info = info.iter().last_data_info();
            let last_data_found = entries.iter().rev().data().next().map(|entry| entry.track);
            (last_data_info == last_data_found).ok_or_else(|| io::Error::new(
                ErrorKind::InvalidData,
                format!("info entry claims last data track is number {last_data_info:?}, but last audio track found is number {last_data_found:?}")
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usable_in_const_context() {
        // compile-time proof: these only compile if TrackNo is usable in const contexts
        const ONE: TrackNo = TrackNo::from(1);
        const IS_NORMAL: bool = ONE.is_normal();
        const IS_SPECIAL: bool = TrackNo(0xAA).is_special();
        const CAST: usize = ONE.cast();
        const AS_U8: u8 = u8::from(ONE);
        const EQ: bool = ONE == TrackNo(1);
        const NE: bool = ONE != TrackNo(2);
        const LT: bool = ONE < TrackNo(2);
        const MIN_IS_NORMAL: bool = TrackNo::MIN_NORMAL.is_normal();
        const MAX_IS_NORMAL: bool = TrackNo::MAX_NORMAL.is_normal();
        const ZERO_IS_SPECIAL: bool = TrackNo(0).is_special();
        const HUNDRED_IS_SPECIAL: bool = TrackNo(100).is_special();

        // cross-check the const-evaluated values against runtime evaluation
        let one = TrackNo(1);
        assert_eq!(IS_NORMAL, one.is_normal());
        assert_eq!(IS_SPECIAL, TrackNo(0xAA).is_special());
        assert_eq!(CAST, one.cast::<usize>());
        assert_eq!(AS_U8, u8::from(one));
        assert_eq!(EQ, one == TrackNo(1));
        assert_eq!(NE, one != TrackNo(2));
        assert_eq!(LT, one < TrackNo(2));
        assert_eq!(MIN_IS_NORMAL, TrackNo::MIN_NORMAL.is_normal());
        assert_eq!(MAX_IS_NORMAL, TrackNo::MAX_NORMAL.is_normal());
        assert_eq!(ZERO_IS_SPECIAL, TrackNo(0).is_special());
        assert_eq!(HUNDRED_IS_SPECIAL, TrackNo(100).is_special());
    }

    #[test]
    fn new_normal_only_accepts_normal_track_numbers() {
        assert_eq!(TrackNo::new_normal(0_u8), None);
        assert_eq!(TrackNo::new_normal(1_u8), Some(TrackNo::MIN_NORMAL));
        assert_eq!(TrackNo::new_normal(42_u8), Some(TrackNo(42)));
        assert_eq!(TrackNo::new_normal(99_u8), Some(TrackNo::MAX_NORMAL));
        assert_eq!(TrackNo::new_normal(100_u8), None);
        assert_eq!(TrackNo::new_normal(0xAA_u8), None);

        // wider integers are accepted via TryFrom, rejected if they do not fit in a u8
        assert_eq!(TrackNo::new_normal(1_u16), Some(TrackNo(1)));
        assert_eq!(TrackNo::new_normal(99_u32), Some(TrackNo(99)));
        assert_eq!(TrackNo::new_normal(100_u16), None);
        assert_eq!(TrackNo::new_normal(256_u16), None);
        assert_eq!(TrackNo::new_normal(u16::MAX), None);
        assert_eq!(TrackNo::new_normal(-1_i32), None);
    }
}
