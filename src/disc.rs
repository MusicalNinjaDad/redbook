//! Disc metadata and MusicBrainz integration
//!
//! This module provides the [`Disc`] struct for representing a physical CD,
//! including its table of contents, tracks, and metadata retrieved from
//! MusicBrainz and CoverArtArchive.
//!
//! # Tracing
//!
//! This module emits the following spans:
//! - `Disc::new` (INFO): Disc creation with `track_count` field
//! - `Disc::track` (DEBUG): Track lookup with `track_number` field
//! - `Disc::tracks` (DEBUG): Track iteration
//! - `Disc::set_release` (DEBUG): Release selection with `index` field
//! - `Disc::tag_for` (DEBUG): Tag generation with `track_number` and `title` fields
//! - `update_musicbrainz` (INFO): MusicBrainz update with `discid` field
//! - `update_cover_art` (INFO): Cover art retrieval
//!
//! Events:
//! - `musicbrainz_retrieved` (INFO): On successful MusicBrainz lookup with `releases` count
//! - `coverart_retrieved` (INFO): On successful cover art retrieval with `size_bytes` field
//! - `coverart_failed` (WARN): On cover art retrieval failure with `url`, `status`, and `reason` fields

use std::{
    fmt::Display,
    fs::File,
    io::{self, Write},
    path::{Path, PathBuf},
};

use cdtoc::Toc;
use metaflac::block::{Picture, PictureType, VorbisComment};
use musicbrainz_rs::Fetch;

use crate::{
    Frame, Msf, Track,
    musicbrainz::{ArtistCreditsExt, Discid, Release, VorbisTagExt},
    tagging::PictureExt,
};

#[derive(Debug, Clone, PartialEq)]
/// Represents a physical CD with its table of contents, tracks, and metadata.
///
/// This is the main starting point for all data and actions you take on the CD itself.
/// It is usually stored in some kind of drive struct which implements
/// [`AudioCdExt`][crate::AudioCdExt] and therefore knows how to get data from the CD.
///
/// # Example
///
/// ```no_run
/// use cdtoc::Toc;
/// use redbook::{Disc, Frame, Track};
///
/// let toc = Toc::from_cdtoc("4+96+2D2B+6256+B327+D84A").unwrap();
/// let tracks = [
///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
///     Track::new(2, Frame::new(0x2d2b), Frame::new(0x6256 - 0x2d2b), None),
///     Track::new(3, Frame::new(0x6256), Frame::new(0xb327 - 0x6256), None),
///     Track::new(4, Frame::new(0xb327), Frame::new(0xd84a - 0xb327), None),
/// ];
/// let leadout = Frame::new(0xd84a);
///
/// // validates TOC, Tracks & leadout match
/// let mut disc = Disc::new(toc, tracks, leadout)?;
///
/// // Attempt to retrieve metadata from MusicBrainz
/// disc.update_musicbrainz()?;
///
/// // Select a specific release & attempt to get the cover art from CoverArtArchive
/// disc.set_release(Some(2)).update_cover_art()?;
///
/// assert_eq!(disc.title().unwrap(), "Iron-Oxide");
/// assert_eq!(disc.main_artist().unwrap(), "Ferris");
/// let track1_details = disc.tag_for(1).unwrap();
/// # std::io::Result::Ok(())
/// ```
pub struct Disc {
    /// The table of contents for the CD.
    toc: Toc,
    /// The list of tracks on the CD.
    tracks: Vec<Track<'static>>,
    /// The leadout frame position.
    leadout: Frame,
    /// MusicBrainz metadata for this disc, if available.
    musicbrainz: Option<Discid>,
    /// Selected release index from musicbrainz.releases.
    ///
    /// Use [`set_release()`][Self::set_release] to set and [`release()`][Self::release] to get.
    ///
    /// - `None` if no selection made yet
    /// - `Some(0)` if no data present
    /// - `Some(0)` if first release selected
    /// - `Some(n)` if specific release selected
    release_index: Option<usize>,
    /// The 0-indexed disc number. Needed for multi-disc releases.
    ///
    /// Automatically set to `Some(0)` for single-disc releases.
    ///
    /// - `None` if no release is selected
    /// - `Some(n)` if release is selected
    disc_index: Option<usize>,
    /// Cached cover art if available
    coverart: Option<Picture>,
}

#[derive(Debug)]
/// Errors that can occur when creating a [`Disc`].
///
/// These errors are returned by [`Disc::new()`][Self::new] when the provided data is inconsistent.
///
/// # Examples
///
/// ```rust
/// use redbook::disc::DiscError;
///
/// // Leadout frame doesn't match TOC
/// let result: Result<(), DiscError> = Err(DiscError::IncorrectLeadout);
/// assert!(matches!(result, Err(DiscError::IncorrectLeadout)));
///
/// // Track MSF or duration doesn't match TOC entry
/// let result: Result<(), DiscError> = Err(DiscError::TocMismatch);
/// assert!(matches!(result, Err(DiscError::TocMismatch)));
/// ```
pub enum DiscError {
    /// The leadout frame does not match the TOC's leadout value.
    IncorrectLeadout,
    /// A track's MSF or duration does not match the corresponding TOC entry.
    TocMismatch,
}

impl std::error::Error for DiscError {}

impl Display for DiscError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DiscError::IncorrectLeadout => write!(f, "incorrect leadout"),
            DiscError::TocMismatch => write!(f, "TOC mismatch"),
        }
    }
}

impl From<DiscError> for io::Error {
    fn from(error: DiscError) -> Self {
        io::Error::new(io::ErrorKind::InvalidData, error)
    }
}

impl Disc {
    /// Creates a new [`Disc`] from a table of contents, tracks, and leadout frame.
    ///
    /// # Notes
    ///
    /// This constructor validates that all provided data is consistent:
    /// - The leadout frame must match the TOC's leadout value
    /// - Each track's start position (MSF) must match its corresponding TOC entry
    /// - Each track's duration must match the duration calculated from its TOC entry
    ///
    /// # Errors
    ///
    /// - Returns [`DiscError::IncorrectLeadout`] if the leadout doesn't match the TOC.
    /// - Returns [`DiscError::TocMismatch`] if any track doesn't match its TOC entry.
    ///
    /// # Examples
    ///
    /// ```
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    ///
    /// let disc = Disc::new(toc, tracks, leadout).unwrap();
    /// assert_eq!(disc.tracks().count(), 1);
    /// ```
    ///
    /// For a more complete example showing how to set metadata retrieval after initialisation,
    /// see the [`Disc`] struct documentation.
    ///
    /// # TODO
    ///
    /// - Add `new_unchecked()` and/or handle mixed-mode CDs as per [`TOC-string definition`]
    ///   (https://forum.dbpoweramp.com/forum/other-topics/developers-corner/16082-flac-ogg-vorbis-storage-of-cdtoc#post16082)
    ///   
    pub fn new<T: IntoIterator<Item = Track<'static>>>(
        toc: Toc,
        tracks: T,
        leadout: Frame,
    ) -> Result<Self, DiscError> {
        let tracks: Vec<_> = tracks.into_iter().collect();
        let _span = tracing::info_span!("Disc::new", track_count = tracks.len());
        let _enter = _span.enter();

        if toc.leadout() != leadout.as_usize() as u32 {
            return Err(DiscError::IncorrectLeadout);
        }

        for track in tracks.iter() {
            let track_number = track.toc_entry.track as usize;
            let toc_track = toc
                .audio_track(track_number)
                .ok_or(DiscError::TocMismatch)?;

            let (min, sec, frame) = toc_track.msf();
            if Msf::new(min as u8, sec, frame) != Msf::from(track.toc_entry.start) {
                return Err(DiscError::TocMismatch);
            }

            let (d, h, min, sec, frame) = toc_track.duration().dhmsf();
            let min = (((d * 24) + h as u64) * 60) + min as u64;
            if Msf::new(min as u8, sec, frame) != Msf::from(track.duration) {
                return Err(DiscError::TocMismatch);
            }
        }

        Ok(Self {
            toc,
            tracks,
            leadout,
            musicbrainz: None,
            release_index: None,
            disc_index: None,
            coverart: None,
        })
    }

    /// Get the selected release.
    ///
    /// # Note
    ///
    /// Where possible the release is automatically selected when you call
    /// [`update_musicbrainz()`][Self::update_musicbrainz] or
    /// [`set_musicbrainz()`][Self::set_musicbrainz]
    ///
    /// Usually there are multiple releases and calls to `release` will return `None`.
    /// In these cases you must use [`set_release()`][Self::set_release] to manually select a specific release
    ///
    /// # See also
    ///
    /// [`update_musicbrainz()`][Self::update_musicbrainz], [`set_musicbrainz()`][Self::set_musicbrainz], [`set_release()`][Self::set_release]
    ///
    /// # Example
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// // For rare discs with a single release, release is already selected
    /// // usually discs have multiple releases, and you need to select one
    /// if disc.release().is_none() {
    ///     // ... some logic to identify the correct release
    ///     let correct_release = Some(2);
    ///     disc.set_release(correct_release);
    /// }
    ///
    /// let release = disc.release();
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn release(&self) -> Option<&Release> {
        self.musicbrainz
            .as_ref()?
            .releases
            .as_ref()?
            .get(self.release_index?)
    }

    /// Get the full MusicBrainz data as loaded by [`update_musicbrainz()`][Self::update_musicbrainz].
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// if let Some(discid) = disc.musicbrainz() {
    ///     println!("Disc ID: {}", discid.id);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn musicbrainz(&self) -> Option<&Discid> {
        self.musicbrainz.as_ref()
    }

    /// Get the title of the CD.
    ///
    /// # Notes
    ///
    /// The title requires a valid release to have been selected, see [`set_release()`][Self::set_release] and
    /// [`update_musicbrainz()`][Self::update_musicbrainz] for details.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// if let Some(title) = disc.title() {
    ///     println!("Album title: {}", title);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn title(&self) -> Option<String> {
        self.release().map(|release| release.title.clone())
    }

    /// Get the main artist for the CD.
    ///
    /// # Notes
    ///
    /// Requires a valid release to have been selected, see [`set_release()`][Self::set_release] and
    /// [`update_musicbrainz()`][Self::update_musicbrainz] for details.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// if let Some(artist) = disc.main_artist() {
    ///     println!("Main artist: {}", artist);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn main_artist(&self) -> Option<String> {
        self.release()
            .and_then(|release| release.artist_credit.main_artist())
    }

    /// Get the disc number as a string.
    ///
    /// # Notes
    ///
    /// Returns the human-readable disc number (e.g., "1", "2"). Requires a valid release
    /// and disc index to have been selected, see [`set_release()`][Self::set_release], [`update_musicbrainz()`][Self::update_musicbrainz],
    /// and [`reset_disc_index()`][Self::reset_disc_index] for details.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// if let Some(disc_num) = disc.disc_number() {
    ///     println!("Disc number: {}", disc_num);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn disc_number(&self) -> Option<String> {
        self.release()
            .and_then(|release| release.media.as_ref())
            .and_then(|all_media| all_media.get(self.disc_index?))
            .and_then(|media| media.position.as_ref().map(ToString::to_string))
    }

    /// Get a track by number with metadata from the selected release, if available.
    ///
    /// # Notes
    ///
    /// Holding on to the returned track will block any mutation to `self` in order
    /// to maintain validity of the metadata. Modifying the returned track will NOT
    /// modify the copy stored in `self`.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let disc = Disc::new(toc, tracks, leadout).unwrap();
    ///
    /// // Get the first track
    /// if let Some(track) = disc.track(1) {
    ///     println!("Track 1 number: {}", track.track_number());
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn track(&self, track_number: usize) -> Option<Track<'_>> {
        let _span = tracing::debug_span!("Disc::track", track_number = track_number);
        let _enter = _span.enter();
        let mut track = self.tracks.get(track_number - 1).cloned()?;
        track.meta = self
            .release()
            .and_then(|r| r.media.as_ref())
            .and_then(|all_media| all_media.get(self.disc_index?))
            .and_then(|media| media.tracks.as_ref())
            .and_then(|tracks| {
                tracks
                    .iter()
                    .find(|trk| trk.number.parse() == Ok(track_number))
            });
        Some(track)
    }

    /// Iterate over all tracks with metadata from the selected release.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let disc = Disc::new(toc, tracks, leadout).unwrap();
    ///
    /// // Iterate through all tracks
    /// for track in disc.tracks() {
    ///     println!("Track: {}", track.track_number());
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn tracks(&self) -> Tracks<'_> {
        let _span = tracing::debug_span!("Disc::tracks", track_count = self.tracks.len());
        let _enter = _span.enter();
        Tracks { disc: self, i: 0 }
    }

    /// Set the selected release by index, or reset to `None`.
    ///
    /// Providing an invalid index will make no change. Returns `self` for chaining.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// // Select the second release (if available)
    /// disc.set_release(Some(1));
    ///
    /// // Or reset the selection
    /// disc.set_release(None);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn set_release(&mut self, index: Option<usize>) -> &mut Self {
        let _span = tracing::debug_span!("Disc::set_release", index = ?index);
        let _enter = _span.enter();
        self.release_index = match index {
            Some(index)
                if self
                    .musicbrainz
                    .as_ref()
                    .and_then(|disc_id| disc_id.releases.as_ref())
                    .and_then(|release| release.get(index))
                    .is_some() =>
            {
                Some(index)
            }
            _ => None,
        };
        let _ = self.reset_disc_index();
        self
    }

    /// Reset the disc index based on the selected release's media.
    ///
    /// Automatically sets to `Some(0)` for single-disc releases, or `None` for multi-disc
    /// releases where the disc must be identified manually.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// // Reset disc index (automatically set for single-disc)
    /// let disc_index = disc.reset_disc_index();
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn reset_disc_index(&mut self) -> Option<usize> {
        let release = self.release()?;
        let media = release.media.as_ref()?;

        self.disc_index = match media.len() {
            ..=1 => Some(0),
            _ => self.find_disc_index_from_media(),
        };
        self.disc_index
    }

    /// Find which media entry in the release matches this disc's TOC.
    ///
    /// For multi-disc releases, each media entry represents one disc.
    /// We match by comparing track offsets from the Discid with calculated offsets from media.
    ///
    /// # Returns
    ///
    /// - `Some(index)` if exactly one media entry matches based on track offsets
    /// - `None` if no matches or multiple matches (ambiguous)
    fn find_disc_index_from_media(&self) -> Option<usize> {
        let release = self.release()?;
        let offsets = self.toc.audio_sectors();
        let matches: Vec<_> = release
            .media
            .as_ref()
            .map(|all_media| {
                all_media.iter().filter(|media| {
                    media
                        .discs
                        .as_ref()
                        .and_then(|discs| discs.iter().find(|disc| disc.offsets == offsets))
                        .is_some()
                })
            })?
            .collect();

        match matches.len() {
            1 => release.media.as_ref().and_then(|all_media| {
                all_media.iter().position(|media| {
                    Some(&media.id) == matches.first().map(|matched_media| &matched_media.id)
                })
            }),
            _ => None,
        }
    }

    /// Set MusicBrainz data directly.
    ///
    /// Replaces any existing MusicBrainz data with the provided [`Discid`] and
    /// automatically selects a release if there is exactly one available. Returns `self` for chaining.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    /// use musicbrainz_rs::entity::discid::Discid;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    ///
    /// // Assuming you have a Discid from MusicBrainz
    /// // let discid: Discid = ...;
    /// // disc.set_musicbrainz(discid);
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn set_musicbrainz(&mut self, discid: Discid) -> &mut Self {
        self.musicbrainz = Some(discid);
        self.release_index = match self
            .musicbrainz
            .as_ref()
            .and_then(|mb| mb.releases.as_ref())
        {
            None => Some(0),
            Some(releases) if releases.is_empty() => Some(0),
            Some(releases) if releases.len() == 1 => Some(1),
            _ => None,
        };
        self
    }

    /// Update MusicBrainz metadata for this disc.
    ///
    /// Performs a lookup using the disc's TOC to generate a DiscID. A single TOC may match
    /// multiple releases (or theoretically multiple discs within a release). See
    /// [Disc ID documentation](https://musicbrainz.org/doc/Disc_ID) for details.
    ///
    /// Release and disc index are automatically selected when uniquely identifiable.
    /// Otherwise, the relevant values will be `None` and must be set manually using
    /// [`set_release()`][Self::set_release] and [`reset_disc_index()`][Self::reset_disc_index].
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the network request or parsing fails.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    ///
    /// // Fetch metadata from MusicBrainz
    /// disc.update_musicbrainz()?;
    ///
    /// // Check if release was auto-selected
    /// if disc.release().is_none() {
    ///     // Multiple releases found, need to select one
    ///     disc.set_release(Some(0));
    /// }
    ///
    /// // Check if disc index was auto-selected
    /// if disc.disc_index().is_none() {
    ///     // Multi-disc release, need to identify which disc
    ///     disc.reset_disc_index();
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn update_musicbrainz(&mut self) -> io::Result<()> {
        let discid = self.toc.musicbrainz_id().to_string();
        let _span = tracing::info_span!("update_musicbrainz", discid = %discid);
        let _enter = _span.enter();

        let mut mb_stuff = Discid::fetch();
        mb_stuff.id(&discid).with_artists().with_recordings();

        let discid = mb_stuff.execute().map_err(io::Error::other)?;

        self.set_musicbrainz(discid);

        if let Some(ref mb) = self.musicbrainz {
            let release_count = mb.releases.as_ref().map(|r| r.len()).unwrap_or(0);
            tracing::info!(releases = release_count, "musicbrainz_retrieved");
        }
        Ok(())
    }

    /// Fetch cover art from CoverArtArchive for the selected release.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if no release is selected or the download fails.
    ///
    /// # Notes
    ///
    /// Cover art is cached and can be retrieved with [`cover_art()`][Self::cover_art].
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// // Fetch cover art
    /// disc.update_cover_art()?;
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn update_cover_art(&mut self) -> io::Result<()> {
        let release_mbid = self
            .release()
            .ok_or_else(|| io::Error::other("No releases found"))?
            .id
            .clone();

        let client = reqwest::blocking::Client::new();
        let url = format!("https://coverartarchive.org/release/{release_mbid}/front");
        let response = client
            .get(&url)
            .header("User-Agent", "splurt/0.1.0")
            .send()
            .map_err(io::Error::other)?;

        let _span = tracing::info_span!("update_cover_art", url = %url);
        let _enter = _span.enter();

        if response.status().is_success() {
            let headers: String = response
                .headers()
                .iter()
                .map(|(key, value)| format!("{key}: {value:?}\n"))
                .collect();
            tracing::debug!(headers, "coverart");
            let image = response.bytes().map_err(io::Error::other)?;
            let cover = Picture::from_jpeg(PictureType::CoverFront, "Front Cover", image.clone());
            self.coverart = Some(cover);
            tracing::info!(size_bytes = image.len(), "coverart_retrieved");
        } else {
            let status = response.status();
            let reason = response.text().ok();
            tracing::warn!(url = %url, status = %status, reason = ?reason, "coverart_failed");
        }
        Ok(())
    }

    /// Get the cached cover art, if available.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    /// disc.update_cover_art()?;
    ///
    /// if let Some(cover) = disc.cover_art() {
    ///     println!("Found cover art: {:?}", cover.picture_type);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn cover_art(&self) -> Option<&Picture> {
        self.coverart.as_ref()
    }

    /// Get the 0-indexed disc number within a multi-disc release, if available.
    ///
    /// # Notes
    ///
    /// Requires a valid release and disc index to have been selected, see [`set_release()`][Self::set_release],
    /// [`update_musicbrainz()`][Self::update_musicbrainz], and [`reset_disc_index()`][Self::reset_disc_index] for details.
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// if let Some(index) = disc.disc_index() {
    ///     println!("Disc index: {}", index);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn disc_index(&self) -> Option<usize> {
        self.disc_index
    }

    /// Save the cover art to a directory.
    ///
    /// Saves the cached cover art as "front.jpeg" in the specified directory.
    ///
    /// # Returns
    ///
    /// - `None` if no cover art is available
    /// - `Some(Ok(path))` with the absolute path on success
    /// - `Some(Err(error))` if saving failed
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    /// use std::path::PathBuf;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    /// disc.update_cover_art()?;
    ///
    /// // Save cover art to current directory
    /// if let Some(Ok(path)) = disc.save_cover_art(".") {
    ///     println!("Cover art saved to: {:?}", path);
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    #[must_use = "may be `Some(Err(_))`"]
    pub fn save_cover_art<P: AsRef<Path>>(&self, directory: P) -> Option<io::Result<PathBuf>> {
        let data = &self.cover_art()?.data;
        let written_to_path = try {
            let path = directory
                .as_ref()
                .to_owned()
                .join("front.jpeg")
                .absolute()?;
            let mut cover = File::create_new(&path)?;
            cover.write_all(data)?;
            path
        };
        Some(written_to_path)
    }

    /// Generate Vorbis comments for a track.
    ///
    /// Creates a [`VorbisComment`] with track and album metadata including MusicBrainz identifiers.
    ///
    /// # Returns
    ///
    /// - `None` if the track number is invalid
    /// - `Some(VorbisComment)` populated with metadata
    ///
    /// # Examples
    ///
    /// ```rust, no_run
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    /// use metaflac::block::VorbisComment;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
    /// ];
    /// let leadout = Frame::new(0x2d2b);
    /// let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// // Generate tags for the first track
    /// if let Some(tags) = disc.tag_for(1) {
    ///     // Use tags for encoding
    ///     let _title = tags.title();
    /// }
    /// # Ok::<(), std::io::Error>(())
    /// ```
    pub fn tag_for(&self, track_number: usize) -> Option<VorbisComment> {
        let _span = tracing::debug_span!("Disc::tag_for", track_number = track_number);
        let _enter = _span.enter();
        let mut vorbis = VorbisComment::new();
        let track = self.track(track_number)?;

        vorbis.set_track(track.track_number() as u32);

        if let Some(id) = track.windows_identifier {
            vorbis.set("WINDOWS_IDENTIFIER", vec![id.to_string()])
        }

        if let Some(release) = self.release() {
            vorbis.set_album(vec![release.title.clone()]);
            vorbis.set("MUSICBRAINZ_ALBUMID", vec![release.id.clone()]);

            vorbis.set_album_artist(release.artist_credit.artist_names().collect());
            vorbis.set(
                "MUSICBRAINZ_ALBUMARTISTID",
                release.artist_credit.artist_ids().collect(),
            );

            let release_date = release
                .date
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            let release_year = release
                .date
                .as_ref()
                .and_then(|date| date.year())
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default();
            vorbis.set("RELEASEDATE", vec![release_date]);
            vorbis.set("RELEASEYEAR", vec![release_year]);

            vorbis.set(
                "RELEASECOUNTRY",
                vec![release.country.clone().unwrap_or_default()],
            );
            release.status.as_ref().unwrap().extend_vorbis(&mut vorbis);

            vorbis.set("BARCODE", vec![release.barcode.clone().unwrap_or_default()]);

            if let Some(media_list) = release.media.as_ref() {
                let total_discs = media_list.len();
                vorbis.set("TOTALDISCS", vec![total_discs.to_string()]);
                vorbis.set("DISCTOTAL", vec![total_discs.to_string()]);
                if let Some(track_count) = media_list.first().map(|media| media.track_count) {
                    vorbis.set("TOTALTRACKS", vec![track_count.to_string()]);
                    vorbis.set("TRACKTOTAL", vec![track_count.to_string()]);
                };
            }
            if let Some(disc_number) = self.disc_index {
                vorbis.set("DISCNUMBER", vec![(disc_number + 1).to_string()]);
            }
            vorbis.set(
                "MEDIA",
                vec![
                    release
                        .media
                        .as_ref()
                        .and_then(|all_media| all_media.first())
                        .and_then(|media| media.format.clone())
                        .unwrap_or_default(),
                ],
            );

            release
                .text_representation
                .as_ref()
                .and_then(|text_rep| text_rep.script.as_ref())
                .unwrap()
                .extend_vorbis(&mut vorbis);

            if let Some(meta) = track.meta() {
                vorbis.set_title(vec![track.title().clone().unwrap_or_default()]);

                vorbis.set("MUSICBRAINZ_TRACKID", vec![meta.id.clone()]);

                let track_artists = meta
                    .artist_credit
                    .as_ref()
                    .or(release.artist_credit.as_ref());
                vorbis.set_artist(track_artists.artist_names().collect());

                let original_date = meta
                    .recording
                    .as_ref()
                    .and_then(|recording| recording.first_release_date.clone())
                    .unwrap_or_default();
                let original_year = original_date.year().unwrap_or_default().to_string();
                vorbis.set("ORIGINALDATE", vec![original_date]);
                vorbis.set("ORIGINALYEAR", vec![original_year]);
            }
        }

        Some(vorbis)
    }
}

#[derive(Debug)]
/// An iterator over the tracks of a [`Disc`].
///
/// Created by [`Disc::tracks()`][Self::tracks]. Each track yielded by this iterator has its
/// metadata populated from the selected release, if available.
///
/// # Notes
///
/// The iterator borrows from the [`Disc`] it was created from, so the lifetime
/// of the tracks is tied to the lifetime of the disc reference.
///
/// # Examples
///
/// ```rust, no_run
/// use redbook::{Disc, Track, Frame};
/// use cdtoc::Toc;
///
/// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
/// let tracks = [
///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96), None),
/// ];
/// let leadout = Frame::new(0x2d2b);
/// let disc = Disc::new(toc, tracks, leadout).unwrap();
///
/// // Iterate through all tracks
/// for track in disc.tracks() {
///     println!("Track: {}", track.track_number());
/// }
/// # Ok::<(), std::io::Error>(())
/// ```
pub struct Tracks<'meta> {
    /// Reference to the disc being iterated.
    disc: &'meta Disc,
    /// Current iteration index.
    i: usize,
}

impl<'meta> Iterator for Tracks<'meta> {
    type Item = Track<'meta>;

    fn next(&mut self) -> Option<Self::Item> {
        // disc.track() uses 1-indexing so we can be very lazy
        self.i += 1;
        self.disc.track(self.i)
    }
}

impl<'m> ExactSizeIterator for Tracks<'m> {
    fn len(&self) -> usize {
        self.disc.tracks.len()
    }

    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::albums::TestAlbum::{self, *};
    use rstest::rstest;

    // TODO - validate with minimal track info

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    fn new(#[case] album: TestAlbum) {
        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();

        let disc = Disc::new(toc, tracks, leadout).unwrap();
        assert_eq!(disc.toc, album.expected_toc());
        assert_eq!(
            disc.tracks().collect::<Vec<_>>(),
            album.expected_tracks_minimal()
        );
        assert_eq!(disc.leadout, album.expected_leadout());
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    fn identify_disc_index(#[case] album: TestAlbum) {
        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks, leadout).unwrap();
        disc.set_musicbrainz(musicbrainz);
        disc.set_release(Some(album.release()));
        assert_eq!(disc.disc_index(), album.expected_disc_index());
    }

    #[test]
    fn set_release_invalid() {
        let album = DefinitelyMaybe;
        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks, leadout).unwrap();
        disc.set_musicbrainz(musicbrainz);

        disc.set_release(Some(999));
        assert!(disc.release_index.is_none());
    }

    #[test]
    fn set_release_no_musicbrainz() {
        let album = DefinitelyMaybe;
        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();

        let mut disc = Disc::new(toc, tracks, leadout).unwrap();

        disc.set_release(Some(0));
        assert!(disc.release_index.is_none());
    }
}
