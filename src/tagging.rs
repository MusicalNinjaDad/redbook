#![expect(missing_docs, reason = "needs update")]
//! Tagging utilities for FLAC metadata
//!
//! # Tracing
//!
//! This module currently does not emit tracing spans as it only contains
//! trait implementations for metadata manipulation.

use metaflac::block::{Picture, PictureType, VorbisComment};
use musicbrainz_rs::entity::{
    artist_credit::ArtistCredit,
    release::{Release, ReleaseStatus, Track},
    release_scripts::ReleaseScript,
};
use zune_jpeg::{JpegDecoder, zune_core::bytestream::ZCursor};

pub trait ArtistCreditsExt {
    fn artist_names(&self) -> impl Iterator<Item = String>;
    fn artist_ids(&self) -> impl Iterator<Item = String>;
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

pub trait PictureExt {
    fn from_jpeg<B: AsRef<[u8]>, S: ToString>(
        picture_type: PictureType,
        description: S,
        data: B,
    ) -> Self;
}

impl PictureExt for Picture {
    fn from_jpeg<B: AsRef<[u8]>, S: ToString>(
        picture_type: PictureType,
        description: S,
        data: B,
    ) -> Self {
        let mut jpg_info = JpegDecoder::new(ZCursor::from(&data));

        #[expect(unused_must_use, reason = "TODO: change to try_from_jpeg")]
        jpg_info.decode_headers();

        let width = jpg_info.info().unwrap().width as u32;
        let height = jpg_info.info().unwrap().height as u32;

        Picture {
            picture_type,
            mime_type: "image/jpeg".to_string(),
            description: description.to_string(),
            width,
            height,
            depth: 24,
            num_colors: 0,
            data: data.as_ref().to_vec(),
        }
    }
}

pub trait VorbisTagExt {
    const KEY: &'static str;
    fn extend_vorbis(&self, vorbis: &mut VorbisComment);
}

impl VorbisTagExt for ReleaseStatus {
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

impl VorbisTagExt for ReleaseScript {
    const KEY: &'static str = "SCRIPT";

    fn extend_vorbis(&self, vorbis: &mut VorbisComment) {
        vorbis.set(Self::KEY, vec![self.code()])
    }
}
