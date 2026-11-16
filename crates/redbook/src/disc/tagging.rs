//! Tagging helpers for FLAC metadata, shared by [`Disc`][crate::Disc] tag generation and
//! [`RippedTrack`][crate::RippedTrack] construction.
//!
//! This module is crate-internal: nothing here is part of the public API. File-naming logic
//! used when saving a [`RippedTrack`][crate::RippedTrack] lives in
//! [`ripped_track`][crate::ripped_track] instead.

use std::{fmt::Display, str::FromStr};

use metaflac::block::{Picture, PictureType, VorbisComment};
use musicbrainz_rs::entity::{
    artist_credit::ArtistCredit,
    release::{Release, ReleaseStatus, Track},
    release_scripts::ReleaseScript,
};
use tracing_result::Trace;
use zune_jpeg::{JpegDecoder, zune_core::bytestream::ZCursor};

use crate::disc::error::UnknownMimeType;

/// Access to the artist names & MusicBrainz ids contained in an artist credit.
pub(super) trait ArtistCreditsExt {
    /// The names of all artists in the credit, in order.
    fn artist_names(&self) -> impl Iterator<Item = String>;
    /// The MusicBrainz ids of all artists in the credit, in order.
    fn artist_ids(&self) -> impl Iterator<Item = String>;
    /// The name of the first (main) artist in the credit, if any.
    fn main_artist(&self) -> Option<String> {
        self.artist_names().nth(0)
    }
}

impl ArtistCreditsExt for Release {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_names()
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_ids()
    }
}

impl ArtistCreditsExt for Track {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_names()
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_ids()
    }
}

impl ArtistCreditsExt for Vec<ArtistCredit> {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.name.clone())
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.artist.id.clone())
    }
}

impl ArtistCreditsExt for &Vec<ArtistCredit> {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.name.clone())
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.artist.id.clone())
    }
}

impl<T: ArtistCreditsExt> ArtistCreditsExt for Option<T> {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.as_ref()
            .map(|credits| credits.artist_names().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.as_ref()
            .map(|credits| credits.artist_ids().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }
}

/// Update an existing [`VorbisComment`] based upon the value of `Self`.
pub(super) trait ExtendVorbisTag {
    /// The vorbis comment key the value is written to.
    const KEY: &'static str;
    /// Set the tag identified by [`KEY`][Self::KEY] on `vorbis`.
    fn extend_vorbis(&self, vorbis: &mut VorbisComment);
}

impl ExtendVorbisTag for ReleaseStatus {
    const KEY: &'static str = "RELEASESTATUS";

    fn extend_vorbis(&self, vorbis: &mut VorbisComment) {
        let value = match self {
            ReleaseStatus::Official => "Official",
            ReleaseStatus::Promotion => "Promotion",
            ReleaseStatus::Bootleg => "Bootleg",
            ReleaseStatus::PseudoRelease => "Pseudo-Release",
            ReleaseStatus::UnrecognizedReleaseStatus => "Other",
            _ => "Other",
        };
        vorbis.set(Self::KEY, vec![value]);
    }
}

impl ExtendVorbisTag for ReleaseScript {
    const KEY: &'static str = "SCRIPT";

    fn extend_vorbis(&self, vorbis: &mut VorbisComment) {
        vorbis.set(Self::KEY, vec![self.code()])
    }
}

/// The image mime types supported for cover art.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImageMimeTypes {
    /// jpeg image data
    Jpeg,
}

impl Display for ImageMimeTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "image/")?;
        match self {
            ImageMimeTypes::Jpeg => write!(f, "jpeg"),
        }
    }
}

impl FromStr for ImageMimeTypes {
    type Err = UnknownMimeType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mimetype = match s {
            "image/jpeg" => ImageMimeTypes::Jpeg,
            _ => Err(UnknownMimeType::from(s)).or_warn("parsing artwork mime type")?,
        };
        Ok(mimetype)
    }
}

/// Construct a [`Picture`] block of `picture_type` from jpeg `data`, reading the image
/// dimensions from the jpeg headers.
pub(crate) fn picture_from_jpeg<B: AsRef<[u8]>, S: ToString>(
    picture_type: PictureType,
    description: S,
    data: B,
) -> Picture {
    let mut jpg_info = JpegDecoder::new(ZCursor::from(&data));

    #[expect(unused_must_use, reason = "TODO: change to try_from_jpeg")]
    jpg_info.decode_headers();

    let width = jpg_info.info().unwrap().width as u32;
    let height = jpg_info.info().unwrap().height as u32;

    Picture {
        picture_type,
        mime_type: ImageMimeTypes::Jpeg.to_string(),
        description: description.to_string(),
        width,
        height,
        depth: 24,
        num_colors: 0,
        data: data.as_ref().to_vec(),
    }
}
