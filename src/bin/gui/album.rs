use musicbrainz_rs::entity::release::Release;
use redbook::tagging::ArtistCreditsExt;
use slint::ToSharedString;

use crate::AlbumDetails;

impl From<&Release> for AlbumDetails {
    fn from(release: &Release) -> Self {
        let artist = release.main_artist().clone().unwrap_or_default();
        let title = release.title.clone();
        let album = [artist, title].join(": ").to_shared_string();
        let comment = release
            .disambiguation
            .as_ref()
            .map(|comment| format!("({comment})"))
            .unwrap_or_default()
            .to_shared_string();
        let date = release.date.clone().unwrap_or_default().to_shared_string();
        let location = release
            .country
            .clone()
            .unwrap_or_default()
            .to_shared_string();
        let barcode = release
            .barcode
            .clone()
            .unwrap_or_default()
            .to_shared_string();
        Self {
            album,
            barcode,
            comment,
            date,
            location,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {

    use redbook::{
        Disc,
        test_fixtures::albums::TestAlbum::{self, *},
    };

    use super::*;

    #[test]
    fn albumdetails() {
        let album: TestAlbum = DefinitelyMaybe;

        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();
        let mut disc = Disc::new(toc, tracks, leadout).unwrap();

        let musicbrainz = album.expected_musicbrainz();
        disc.set_musicbrainz(musicbrainz);
        disc.set_release_index(Some(album.release()));

        let details = AlbumDetails::from(disc.release().unwrap());

        let expected = AlbumDetails {
            album: "Oasis: Definitely Maybe".into(),
            date: "1994-08-30".into(),
            location: "GB".into(),
            barcode: "5017556601693".into(),
            comment: "(Plant MFG pressing)".into(),
            ..Default::default()
        };

        assert_eq!(details.album, expected.album);
        assert_eq!(details.date, expected.date);
        assert_eq!(details.location, expected.location);
        assert_eq!(details.barcode, expected.barcode);
        assert_eq!(details.comment, expected.comment);
    }
}
