//! Disc metadata and MusicBrainz integration
//!
//! Provides the [`Disc`] struct for representing a physical CD, including its table of contents,
//! tracks, and metadata retrieved from MusicBrainz and CoverArtArchive.
use std::{
    collections::HashMap,
    fs::File,
    io::{self, ErrorKind, Write},
    path::{Path, PathBuf},
};

use cdtoc::Toc;
use itertools::Itertools;
use metaflac::block::{Picture, PictureType, VorbisComment};
use musicbrainz_rs::{
    Fetch,
    chrono::NaiveDate,
    entity::{discid::Discid, release::Release},
};
use thread_safely::Context;
use tracing::{debug_span, field::Empty};
use tracing_result::Trace;

use crate::{
    Track,
    tagging::{ArtistCreditsExt, ExtendVorbisTag, PictureExt},
};

#[derive(Debug, Clone, PartialEq)]
/// Represents a logical CD with additional metadata such as track names and cover art.
///
/// This is the main starting point for all data and actions you take on the CD itself.
/// It is usually stored in an [`AudioCd`][crate::AudioCd] which provides access to the physical
/// disc.
///
/// # Example
///
/// ```no_run
/// use cdtoc::Toc;
/// use redbook::{Disc, Frame, Track};
///
/// let toc = Toc::from_cdtoc("4+96+2D2B+6256+B327+D84A").unwrap();
/// let tracks = [
///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96)),
///     Track::new(2, Frame::new(0x2d2b), Frame::new(0x6256 - 0x2d2b)),
///     Track::new(3, Frame::new(0x6256), Frame::new(0xb327 - 0x6256)),
///     Track::new(4, Frame::new(0xb327), Frame::new(0xd84a - 0xb327)),
/// ];
///
/// // validates TOC, Tracks & leadout match
/// let mut disc = Disc::new(toc, tracks)?;
///
/// // Attempt to retrieve metadata from MusicBrainz
/// disc.update_musicbrainz()?;
///
/// // Select a specific release & attempt to get the cover art from CoverArtArchive
/// disc.set_release_by_id(Some("release_id_12938utzqgrbf8")).update_cover_art()?;
///
/// assert_eq!(disc.title().unwrap(), "Iron-Oxide");
/// assert_eq!(disc.main_artist().unwrap(), "Ferris");
/// let track1_details = disc.tag_for(1).unwrap();
/// # std::io::Result::Ok(())
/// ```
pub struct Disc {
    /// The table of contents for the CD.
    toc: Toc,
    /// The list of audio tracks on the CD.
    tracks: Vec<Track<'static>>,
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
    /// Cover art thumbnails for specific releases
    thumbnails: HashMap<String, Picture>,
}

impl Disc {
    /// Creates a new [`Disc`] from a table of contents and an interator of tracks.
    ///
    /// # Notes
    ///
    /// This constructor validates that all provided data is consistent:
    /// - The number of tracks must match between both inputs
    /// - Each track's start frame must match the corresponding TOC entry
    /// - Each track's duration must match the duration calculated from the TOC
    ///
    /// # Examples
    ///
    /// ```
    /// use redbook::{Disc, Track, Frame};
    /// use cdtoc::Toc;
    ///
    /// let toc = Toc::from_cdtoc("1+96+2D2B").unwrap();
    /// let tracks = [
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96)),
    /// ];
    ///
    /// let disc = Disc::new(toc, tracks).unwrap();
    /// assert_eq!(disc.tracks().count(), 1);
    /// ```
    ///
    /// For a more complete example showing how to set metadata retrieval after initialisation,
    /// see the [`Disc`] struct documentation.
    pub fn new<T: IntoIterator<Item = Track<'static>>>(toc: Toc, tracks: T) -> io::Result<Self> {
        let tracks: Vec<_> = tracks.into_iter().collect();
        let _info = tracing::info_span!("Disc::new", track_count = tracks.len()).entered();

        tracks
            .iter()
            .zip_longest(toc.audio_tracks())
            .try_for_each(|zipped| match zipped {
                itertools::EitherOrBoth::Both(track, toc_track) if track == &toc_track => Ok(()),
                itertools::EitherOrBoth::Both(track, toc_track) => Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("track mismatch: toc {toc_track:?} vs tracks {track}"),
                )),
                itertools::EitherOrBoth::Left(track) => Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "provided tracks contains more entries than toc. first additional entry {track}"
                    ),
                )),
                itertools::EitherOrBoth::Right(toc_track) => Err(io::Error::new(
                    ErrorKind::InvalidInput,
                    format!(
                        "toc contains more entries than provided tracks. first additional entry {toc_track:?}"
                    ),
                )),
            }).or_warn("mismatch between provided TOC & provided tracks")?;

        Ok(Self {
            toc,
            tracks,
            musicbrainz: None,
            release_index: None,
            disc_index: None,
            coverart: None,
            thumbnails: HashMap::new(),
        })
    }

    /// Get the Table of Contents in cdtoc format
    pub fn toc(&self) -> &Toc {
        &self.toc
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
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96)),
    /// ];
    /// let mut disc = Disc::new(toc, tracks).unwrap();
    /// disc.update_musicbrainz()?;
    ///
    /// // For rare discs with a single release, release is already selected
    /// // usually discs have multiple releases, and you need to select one
    /// if disc.release().is_none() {
    ///     // ... some logic to identify the correct release
    ///     let correct_release = Some("release_id_13iukhi8i9dsazbn32reg");
    ///     disc.set_release_by_id(correct_release);
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
    pub fn musicbrainz(&self) -> Option<&Discid> {
        self.musicbrainz.as_ref()
    }

    /// Get the title of the CD.
    ///
    /// # Notes
    ///
    /// The title requires a valid release to have been selected, see [`set_release()`][Self::set_release] and
    /// [`update_musicbrainz()`][Self::update_musicbrainz] for details.
    pub fn title(&self) -> Option<String> {
        self.release().map(|release| release.title.clone())
    }

    /// Get the main artist for the CD.
    ///
    /// # Notes
    ///
    /// Requires a valid release to have been selected, see [`set_release()`][Self::set_release] and
    /// [`update_musicbrainz()`][Self::update_musicbrainz] for details.
    pub fn main_artist(&self) -> Option<String> {
        self.release()
            .and_then(|release| release.artist_credit.main_artist())
    }

    /// Get the disc number as a string.
    ///
    /// # Notes
    ///
    /// Returns the human-readable disc number (e.g., "1", "2"). Requires a valid release
    /// and disc index to have been selected, see [`set_release()`][Self::set_release],
    /// [`update_musicbrainz()`][Self::update_musicbrainz], and
    /// [`reset_disc_index()`][Self::reset_disc_index] for details.
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
    pub fn track(&self, track_number: usize) -> Option<Track<'_>> {
        let _debug = tracing::debug_span!("Disc::track", track_number = track_number).entered();
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
    pub fn tracks(&self) -> Tracks<'_> {
        let _debug =
            tracing::debug_span!("Disc::tracks", track_count = self.tracks.len()).entered();
        Tracks { disc: self, i: 0 }
    }

    /// Get the possible releases grouped by title then sorted newest to oldest
    pub fn all_releases(&self) -> Option<Vec<&Release>> {
        let mut releases: Vec<_> = self
            .musicbrainz
            .as_ref()?
            .releases
            .as_ref()?
            .iter()
            .filter(|release| match self.toc.has_data() {
                true => release.has_data(),
                false => release.has_audio_only(),
            })
            .collect();
        releases.sort_by_key(|release| {
            release
                .date
                .as_ref()
                .and_then(|date| date.into_naive_date(1, 1, 1).ok())
                .unwrap_or(
                    // NaiveDate::default() is 1970-01-01 which is unsuitable for older music
                    NaiveDate::from_ymd_opt(1, 1, 1).unwrap(),
                )
        });
        releases.reverse();
        releases.sort_by_key(|release| &release.title);
        Some(releases)
    }

    /// Set the release, or reset to `None`.
    ///
    /// Providing an invalid release will make no change. Returns `self` for chaining.
    pub fn set_release(&mut self, release: Option<&Release>) -> &mut Self {
        let _debug = tracing::debug_span!("Disc::set_release", ?release).entered();
        self.release_index = release.and_then(|release| {
            self.musicbrainz.as_ref().and_then(|mb| {
                mb.releases
                    .as_ref()
                    .and_then(|releases| releases.iter().position(|rel| rel.id == release.id))
            })
        });
        tracing::debug!(set_to_index = ?self.release_index);
        self.reset_disc_index();
        self
    }

    fn release_index_for(&self, release: &Release) -> Option<usize> {
        self.musicbrainz.as_ref().and_then(|mb| {
            mb.releases
                .as_ref()
                .and_then(|releases| releases.iter().position(|rel| rel.id == release.id))
        })
    }

    /// Set the release, or reset to `None`.
    ///
    /// Providing an invalid id will make no change. Returns `self` for chaining.
    pub fn set_release_by_id(&mut self, release: Option<&str>) -> &mut Self {
        let _debug = tracing::debug_span!("Disc::set_release_by_id", ?release).entered();
        self.release_index = release.and_then(|release| {
            self.musicbrainz.as_ref().and_then(|mb| {
                mb.releases
                    .as_ref()
                    .and_then(|releases| releases.iter().position(|rel| rel.id == release))
            })
        });
        self.reset_disc_index();
        tracing::debug!(index = self.release_index, disc = self.disc_index);
        self
    }

    #[cfg(test)]
    fn set_release_index(&mut self, index: Option<usize>) -> &mut Self {
        let _debug = tracing::debug_span!("Disc::set_release_index", index = ?index).entered();
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
        self.reset_disc_index();
        self
    }

    /// Reset the release index based on the musicbrainz data.
    ///
    /// Automatically sets to `Some(0)` for single release discs or where no release data is
    /// available, otherwise sets `None` to signify that a selection needs to be made
    fn reset_release_index(&mut self) -> Option<usize> {
        let _debug = tracing::debug_span!(
            "Disc::reset_release_index",
            release_count = self.all_releases().map(|releases| releases.len())
        )
        .entered();

        match self.all_releases() {
            None => self.release_index = Some(0),
            Some(releases) if releases.is_empty() => self.release_index = Some(0),
            Some(releases) if releases.len() == 1 => {
                self.release_index = self.release_index_for(releases.first().unwrap())
            }
            _ => self.release_index = None,
        };

        tracing::debug!(release_index = self.release_index);

        self.reset_disc_index();
        self.release_index
    }

    /// Reset the disc index based on the selected release's media.
    ///
    /// Automatically sets to `Some(0)` for single-disc releases, or `None` for multi-disc
    /// releases where the disc must be identified manually.
    ///
    /// # TODOs
    /// - TODO #128 Tidy disc index, and release index functions
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
    pub fn set_musicbrainz(&mut self, discid: Discid) -> &mut Self {
        self.musicbrainz = Some(discid);
        self.reset_release_index();
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
    /// [`set_release()`][Self::set_release].
    pub fn update_musicbrainz(&mut self) -> io::Result<()> {
        let discid = self.toc.musicbrainz_id().to_string();

        let _span = tracing::info_span!("update musicbrainz", discid = %discid).entered();

        let mut mb_stuff = Discid::fetch();
        mb_stuff.id(&discid).with_artists().with_recordings();

        let discid = {
            let _debug_with_query =
                tracing::debug_span!("fetching from MusicBrainz", query = ?mb_stuff).entered();
            tracing::trace!("starting to fetch");

            mb_stuff
                .execute()
                .map_err(io::Error::other)
                .or_warn("fetching from MusicBrainz")?
        };

        let _debug_with_id =
            tracing::debug_span!("MusicBrainz retrieved", discid = discid.id).entered();
        tracing::info!(releases = discid.releases.as_ref().map(|r| r.len()).unwrap_or(0));

        self.set_musicbrainz(discid);
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
    pub fn update_cover_art(&mut self) -> io::Result<()> {
        let debug = tracing::debug_span!("", url = Empty, size = Empty, status = Empty).entered();
        tracing::trace!("");
        let release_mbid = self
            .release()
            .ok_or_else(|| io::Error::other("No releases found"))
            .or_warn("")?
            .id
            .clone();

        let client = reqwest::blocking::Client::new();
        let url = format!("https://coverartarchive.org/release/{release_mbid}/front");
        debug.record("url", &url);
        tracing::trace!("");

        let response = client
            .get(&url)
            .header("User-Agent", "splurt/0.1.0")
            .send()
            .map_err(io::Error::other)
            .or_debug("requesting cover art")?;

        let status = response.status();
        debug.record("status", status.to_string());

        if status.is_success() {
            let image = response
                .bytes()
                .map_err(io::Error::other)
                .or_debug("unpacking response")?;

            debug.record("size", image.len());
            tracing::info!("coverart_retrieved");

            self.set_cover_art(image);
        } else {
            let reason = response.text().ok();
            tracing::warn!(name: "coverart failed", reason = ?reason);
        }
        Ok(())
    }

    /// Set the cover art
    ///
    /// # Note:
    /// - Use [clear_cover_art()][Self::clear_cover_art] to set to `None`
    pub fn set_cover_art<B: AsRef<[u8]>>(&mut self, jpeg: B) -> &mut Self {
        let _debug = debug_span!("set_cover_art", size = jpeg.as_ref().len()).entered();
        tracing::debug!("setting cover art");
        let cover = Picture::from_jpeg(PictureType::CoverFront, "Front Cover", jpeg);
        self.coverart = Some(cover);
        self
    }

    /// Clear the cover art
    pub fn clear_cover_art(&mut self) -> &mut Self {
        self.coverart = None;
        self
    }

    /// Get the cached cover art, if available.
    pub fn cover_art(&self) -> Option<&Picture> {
        self.coverart.as_ref()
    }

    /// Add a cover art thumbnail for a given release
    pub fn add_thumbnail(&mut self, release_id: String, image: Picture) -> &mut Self {
        self.thumbnails.insert(release_id, image);
        self
    }

    /// Get the cover art thumbnail for a given release
    pub fn get_thumbnail(&self, release_id: &str) -> Option<&Picture> {
        self.thumbnails.get(release_id)
    }

    /// Get all tumbnails from Coverart Archive
    ///
    /// TODO: #62 Check coverartarchive.front == true, (e.g. Urban Hymns has >=1 missing)
    pub fn update_thumbnails(&mut self, cx: Context<!>) -> io::Result<()> {
        let _main_span = tracing::error_span!("Disc::update_thumbnails").entered();
        let debug_span = tracing::debug_span!("", retrieved = 0).entered();
        tracing::trace!("");

        let mut retrieved = 0;

        let client = reqwest::blocking::Client::new();

        let release_ids: Vec<_> = self
            .all_releases()
            .as_ref()
            .map(|releases| releases.iter().map(|release| release.id.clone()))
            .ok_or_else(|| io::Error::other("No releases found"))
            .or_warn("no releases found")?
            .collect();
        for id in release_ids {
            cx.cancelled()?;
            let debug_loop = tracing::debug_span!(
                "Disc::update_thumbnails",
                url = Empty,
                size = Empty,
                status = Empty,
                release = id,
            )
            .entered();

            let url = format!("https://coverartarchive.org/release/{id}/front-250");
            debug_loop.record("url", &url);

            let response = client
                .get(&url)
                .header("User-Agent", "splurt/0.1.0")
                .send()
                .map_err(io::Error::other)
                .or_debug("requesting cover art")?;

            let status = response.status();
            debug_loop.record("status", status.to_string());
            tracing::trace!("");

            if status.is_success() {
                let image = response
                    .bytes()
                    .map_err(io::Error::other)
                    .or_debug("unpacking response")?;

                debug_loop.record("size", image.len());
                tracing::debug!("thumbnail retrieved");
                self.add_thumbnail(
                    id,
                    Picture::from_jpeg(PictureType::CoverFront, "Front Cover", image),
                );
                retrieved += 1;
                debug_span.record("retrieved", retrieved);
            } else {
                let reason = response.text().ok();
                tracing::warn!(name: "thumbnail failed", reason = ?reason);
            }
        }
        tracing::debug!("finished getting thumbnails");
        Ok(())
    }

    /// Get the 0-indexed disc number within a multi-disc release, if available.
    ///
    /// # Notes
    ///
    /// Requires a valid release and disc index to have been selected, see [`set_release()`][Self::set_release],
    /// [`update_musicbrainz()`][Self::update_musicbrainz], and [`reset_disc_index()`][Self::reset_disc_index] for details.
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
    #[must_use = "may be `Some(Err(_))`"]
    pub fn save_cover_art<P: AsRef<Path>>(&self, directory: P) -> Option<io::Result<PathBuf>> {
        let debug =
            tracing::debug_span!("save_cover_art", location = Empty, size = Empty).entered();

        let data = &self.cover_art()?.data;
        debug.record("size", data.len());

        let directory = directory.as_ref().to_owned();
        debug.record("location", directory.display().to_string());

        let written_to_path = try bikeshed Result<_, _> {
            let path = directory
                .join("front.jpeg")
                .absolute()
                .or_warn("unable to identify where to save cover art")?;
            debug.record("location", path.display().to_string());

            let mut cover = File::create_new(&path).or_warn("saving cover art")?;
            cover.write_all(data).or_warn("saving cover art")?;
            path
        };
        Some(written_to_path)
    }

    /// Generate Vorbis comments for a track. This does not include any related images.
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
    ///     Track::new(1, Frame::new(0x96), Frame::new(0x2d2b - 0x96)),
    /// ];
    /// let mut disc = Disc::new(toc, tracks).unwrap();
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
        let _debug = tracing::debug_span!("Disc::tag_for", track_number = track_number).entered();
        let mut vorbis = VorbisComment::new();
        let track = self.track(track_number)?;

        vorbis.set_track(track.track_number() as u32);

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
            // TODO #130 - validate is index correct, or is there a field for discno?
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
/// Created by [`Disc::tracks()`]. Each track yielded by this iterator has its metadata populated
/// from the selected release, if available.
///
/// # Notes
///
/// The iterator borrows from the [`Disc`] it was created from, so the lifetime
/// of the tracks is tied to the lifetime of the disc reference.
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

trait MediaType {
    fn has_data(&self) -> bool;
    fn has_audio_only(&self) -> bool;
}

impl MediaType for Release {
    fn has_data(&self) -> bool {
        self.media
            .as_ref()
            .and_then(|all_media| all_media.iter().find(|media| media.data_tracks.is_some()))
            .is_some()
    }
    fn has_audio_only(&self) -> bool {
        self.media
            .as_ref()
            .and_then(|all_media| all_media.iter().find(|media| media.data_tracks.is_none()))
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use std::iter::zip;

    use super::*;
    use crate::test_fixtures::albums::TestAlbum::{self, *};
    use rstest::rstest;

    // TODO - validate with minimal track info

    #[test_log::test]
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn new(#[case] album: TestAlbum) {
        let toc = album.expected_toc();
        let tracks = album.expected_audio_tracks_minimal();

        let disc = Disc::new(toc, tracks).unwrap();
        assert_eq!(disc.toc, album.expected_toc());
        assert_eq!(
            disc.tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    #[cfg_attr(miri, ignore = "no unsafe used & super slow on miri")]
    fn identify_disc_index(#[case] album: TestAlbum) {
        let toc = album.expected_toc();
        let tracks = album.expected_audio_tracks_minimal();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks).unwrap();
        disc.set_musicbrainz(musicbrainz);
        disc.set_release_index(Some(album.release()));
        assert_eq!(disc.disc_index(), album.expected_disc_index());
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn all_releases(#[case] album: TestAlbum) {
        let expected_order = album.expected_releases_in_order();
        let toc = album.expected_toc();
        let tracks = album.expected_audio_tracks_minimal();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks).unwrap();
        disc.set_musicbrainz(musicbrainz);
        let ordered: Vec<_> = disc.all_releases().unwrap().into_iter().cloned().collect();
        assert_eq!(ordered, expected_order);
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn set_release_by_ref(#[case] album: TestAlbum) {
        let ordered_releases = album.expected_releases_in_order();
        let expected_order = album.expected_release_order();

        let toc = album.expected_toc();
        let tracks = album.expected_audio_tracks_minimal();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks).unwrap();
        disc.set_musicbrainz(musicbrainz);
        for (release, index) in zip(ordered_releases, expected_order) {
            disc.set_release(Some(&release));
            assert_eq!(disc.release_index, Some(index));
        }
    }

    #[test]
    fn set_release_invalid() {
        let album = DefinitelyMaybe;
        let toc = album.expected_toc();
        let tracks = album.expected_audio_tracks_minimal();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks).unwrap();
        disc.set_musicbrainz(musicbrainz);

        disc.set_release_index(Some(999));
        assert!(disc.release_index.is_none());
    }

    #[test]
    fn set_release_no_musicbrainz() {
        let album = DefinitelyMaybe;
        let toc = album.expected_toc();
        let tracks = album.expected_audio_tracks_minimal();

        let mut disc = Disc::new(toc, tracks).unwrap();

        disc.set_release_index(Some(0));
        assert!(disc.release_index.is_none());
    }

    #[test]
    fn set_single_release() {
        let album = BatOutOfHellRevamped;
        let tracks = album.expected_audio_tracks_minimal();
        let toc = album.expected_toc();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks).unwrap();
        disc.set_musicbrainz(musicbrainz);

        assert_eq!(disc.release_index, Some(0));
        assert_eq!(disc.disc_index, Some(0));
        assert_eq!(
            disc.tracks().next().unwrap().title().unwrap(),
            "Bat Out of Hell"
        );
    }

    #[test_log::test]
    /// This Disc only has one possible release which includes a data track
    fn only_extended_cd() {
        let album = TheBoyWhoKnewTooMuch;
        let tracks = album.expected_audio_tracks_minimal();
        let toc = album.expected_toc();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks).unwrap();
        disc.set_musicbrainz(musicbrainz);
        assert_eq!(disc.release_index, Some(3));
        assert_eq!(disc.disc_index, Some(0));
    }
}
