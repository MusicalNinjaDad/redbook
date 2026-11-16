//! Tagging helpers for FLAC metadata, shared by [`Disc`][crate::Disc] tag generation and
//! [`RippedTrack`][crate::RippedTrack] construction.
//!
//! This module is crate-internal: nothing here is part of the public API. File-naming logic
//! used when saving a [`RippedTrack`][crate::RippedTrack] lives in
//! [`ripped_track`][crate::ripped_track] instead.

use metaflac::block::VorbisComment;
use musicbrainz_rs::entity::{
    artist_credit::ArtistCredit,
    release::{Release, ReleaseStatus, Track},
    release_scripts::ReleaseScript,
};

use crate::tags::vorbis::ExtendVorbisTag;

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
