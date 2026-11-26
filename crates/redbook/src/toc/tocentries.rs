//! Provides [`TocEntries`] the primary view of a Table of Contents with all data which can be
//! provided by SCSI MMC calls

use std::{num::NonZeroUsize, slice};

use cdtoc::Toc;
use itertools::Itertools;
use try_partialord::TrySort;

use crate::disc::Track;

use super::{
    Frame, TocError, TocResult, TrackNo,
    control::{self, Ctrl},
    error::InvalidOrder,
    tocentry::{self, LeadoutEntry, TocEntry, TrackEntry},
};

#[derive(Debug, Clone, PartialEq, Eq)]
/// A collection of [`TocEntry`]s, neatly grouped and sorted.
///
/// # Construction
///
/// Construction is available via `FromIterator<TocEntry> for Result<TocEntries, TocError>`
///
/// # Conversion & compatibility
///
/// Conversion is provided *to* [`cdtoc::Toc`] from both `TocEntries` (consuming) and `&TocEntries`
/// (required data is copy). This is a lossy process.
///
/// No conversion is provided *from* [`cdtoc::Toc`] as data would be missing for [`Control`],
/// session info and the [`InfoEntry`]s
pub struct TocEntries {
    entries: Vec<TocEntry>,
    format: Format,
}

/// Filtering helpers over iterators of [`TocEntry`] references.
///
/// Automatically implemented for every `Iterator<Item = &TocEntry>`.
pub trait TocEntryFilter<'e>: Iterator<Item = &'e TocEntry> + Sized {
    /// Keep only entries with an audio control code (not a data track), via [`Ctrl::is_audio`].
    fn filter_audio(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| entry.is_audio())
    }

    /// Keep only entries with a data control code, via [`Ctrl::is_data`].
    fn filter_data(self) -> impl Iterator<Item = &'e TocEntry> {
        self.filter(|entry| entry.is_data())
    }

    /// Map consecutive track / lead-out entries to [`Track`]s with audio tracks only, where the
    /// duration of each track is calculated from the start position of the next entry.
    ///
    /// # Notes
    /// - When the next entry is a data track, the fixed offset between audio & data sessions is
    ///   subtracted from the duration.
    fn filter_map_tracks(self) -> impl Iterator<Item = Track<'static>> {
        self.filter(|entry| entry.is_track() || entry.is_leadout())
            // .filter_map_windows() does not exist.
            // `map().flatten()` is equivalent - see example in docs for `flatten()`
            .map_windows(|[this_track, next_track]| {
                this_track.is_audio_track().then_some(())?;

                let mut duration = next_track.start()?.saturating_sub(this_track.start()?);

                if next_track.is_data_track() {
                    duration = duration.saturating_sub(TocEntries::OFFSET_BEFORE_DATA)
                }

                Some(Track::new(
                    this_track.trackno()?,
                    this_track.start()?,
                    duration,
                ))
            })
            .flatten()
    }
}

impl<'e, I> TocEntryFilter<'e> for I where I: Iterator<Item = &'e TocEntry> + Sized {}

impl TocEntries {
    /// There is a fixed offset between audio & data sessions
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
        self.iter().filter_map_tracks()
    }

    /// Get a given *audio* track. Returns `None` if the track is not an audio track on this disc.
    pub fn track(&self, track: TrackNo) -> Option<Track<'static>> {
        self.iter()
            .skip_while(|entry| entry.trackno() != Some(track))
            .filter_map_tracks()
            .next()
    }

    /// Is this pair of entries well sorted?
    ///
    /// Primarily designed for used in `.map_windows().try_for_each()`
    pub fn well_sorted(format: Format, entries: &[&TocEntry; 2]) -> Result<(), InvalidOrder> {
        use Format::*;
        use control::TrackKind::*;
        use tocentry::TocEntryKind::*;

        let [this_entry, next_entry] = entries;
        match (
            format,
            this_entry.kind(),
            this_entry.track_kind(),
            next_entry.kind(),
            next_entry.track_kind(),
        ) {
            (B0000, Track, _, Track, _)
            | (B0000, Track, _, Leadout, _)
            | (B0010(_), FirstTrackInfo, Audio, LastTrackInfo, Audio)
            | (B0010(_), LastTrackInfo, Audio, Track, Audio)
            | (B0010(_), Track, Audio, Track, Audio)
            | (B0010(_), Track, Audio, Leadout, Audio)
            | (B0010(_), FirstTrackInfo, Data, LastTrackInfo, Data)
            | (B0010(_), LastTrackInfo, Data, Track, Data)
            | (B0010(_), Track, Data, Track, Data)
            | (B0010(_), Track, Data, Leadout, Data)
            | (B0010(_), Leadout, _, Unknown, _)
            | (B0010(_), Unknown, _, Unknown, _)
            | (B0010(_), Unknown, _, FirstTrackInfo, _) => Ok(()),
            _ => Err(InvalidOrder::new(&format, this_entry, next_entry)),
        }
    }
}

/// Whether only the final leadout is present, or leadouts are available for every session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    /// Summary form TOC
    ///
    /// # Note
    /// - If multiple sessions exist, only the final leadout is available.
    /// - No dedicated information entries are available
    B0000,
    /// Full form TOC.
    ///
    /// # Note
    /// - Every session has a leadout available
    /// - Dedicated information entries are present for each session
    /// - Includes the number of sessions found
    B0010(NonZeroUsize),
}

impl<'entries> IntoIterator for &'entries TocEntries {
    type Item = &'entries TocEntry;

    type IntoIter = std::slice::Iter<'entries, TocEntry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl TryFrom<TocEntries> for Toc {
    type Error = TocError;

    fn try_from(entries: TocEntries) -> Result<Self, Self::Error> {
        Self::try_from(&entries)
    }
}

impl TryFrom<&TocEntries> for Toc {
    type Error = TocError;

    fn try_from(entries: &TocEntries) -> Result<Self, Self::Error> {
        let audio = entries
            .iter()
            .filter_audio()
            .filter_map(TocEntry::trackentry)
            .map(TrackEntry::start)
            .map(Frame::cast)
            .collect();
        let data = entries
            .iter()
            .rev()
            .filter_data()
            .filter_map(TocEntry::trackentry)
            .map(TrackEntry::start)
            .find_map(Frame::cast);
        let leadout = entries
            .iter()
            .rev()
            .filter_map(TocEntry::leadoutentry)
            .map(LeadoutEntry::start)
            .find_map(Frame::cast)
            .ok_or(TocError::NotFound)?;
        Ok(Toc::from_parts(audio, data, leadout)?)
    }
}

impl FromIterator<TocEntry> for TocResult<TocEntries> {
    fn from_iter<I: IntoIterator<Item = TocEntry>>(iter: I) -> Self {
        let _span = tracing::debug_span!("FromIterator<TocEntry> for Result<TocEntries, TocError>");
        use tocentry::FirstLast::*;

        let mut entries: Vec<_> = iter.into_iter().collect();

        (!entries.is_empty()).ok_or(TocError::NotFound)?;

        // INVARIANT: all tracks are expected to be sorted
        entries.try_sort().map_err(|_| TocError::MultipleDiscs)?;

        if let Some(first_audio_info) = entries
            .iter()
            .filter_audio()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), First))
        {
            let first_audio_found = entries
                .iter()
                .filter_audio()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(first_audio_info.trackno()) == first_audio_found).ok_or(
                TocError::InfoMismatch {
                    entry: *first_audio_info,
                    found: first_audio_found,
                },
            )?;
        }

        if let Some(last_audio_info) = entries
            .iter()
            .rev()
            .filter_audio()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), Last))
        {
            let last_audio_found = entries
                .iter()
                .rev()
                .filter_audio()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(last_audio_info.trackno()) == last_audio_found).ok_or(
                TocError::InfoMismatch {
                    entry: *last_audio_info,
                    found: last_audio_found,
                },
            )?;
        }

        if let Some(first_data_info) = entries
            .iter()
            .filter_data()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), First))
        {
            let first_data_found = entries
                .iter()
                .filter_data()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(first_data_info.trackno()) == first_data_found).ok_or(
                TocError::InfoMismatch {
                    entry: *first_data_info,
                    found: first_data_found,
                },
            )?;
        }

        if let Some(last_data_info) = entries
            .iter()
            .rev()
            .filter_data()
            .filter_map(TocEntry::infoentry)
            .find(|e| matches!(e.info_type(), Last))
        {
            let last_data_found = entries
                .iter()
                .rev()
                .filter_data()
                .filter_map(TocEntry::trackentry)
                .map(TrackEntry::trackno)
                .next();
            (Some(last_data_info.trackno()) == last_data_found).ok_or(TocError::InfoMismatch {
                entry: *last_data_info,
                found: last_data_found,
            })?;
        }

        let has_entries_without_session = entries
            .iter()
            .map(|entry| entry.session())
            .find(Option::is_none)
            .is_some();
        let has_session_info = entries
            .iter()
            .map(|entry| entry.session())
            .find(Option::is_some)
            .is_some();

        (has_session_info != has_entries_without_session)
            .ok_or(TocError::InconsistentSessionInfo)?;

        let sessions = has_session_info
            .then(|| entries.iter().map(|entry| entry.session()).dedup().count())
            .and_then(NonZeroUsize::new);

        let leadout_count = entries.iter().filter_map(TocEntry::leadoutentry).count();

        let format = match (sessions, leadout_count) {
            (None, 1) => Format::B0000,
            (None, leadouts) => {
                return Err(TocError::LeadoutMismatch {
                    sessions: None,
                    leadouts,
                });
            }
            (Some(session_count), leadouts) if session_count.get() == leadouts => {
                Format::B0010(session_count)
            }
            (Some(_), leadouts) => {
                return Err(TocError::LeadoutMismatch { sessions, leadouts });
            }
        };

        entries
            .iter()
            .map_windows(|entries| TocEntries::well_sorted(format, entries))
            .try_for_each(|x| x)?;

        let tocentries = TocEntries { entries, format };

        Ok(tocentries)
    }
}
