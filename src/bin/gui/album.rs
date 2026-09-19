use std::io::{self, ErrorKind};

use musicbrainz_rs::entity::release::Release;
use redbook::{Disc, tagging::ArtistCreditsExt};
use slint::{Image, ModelRc, ToSharedString};
use tracing::debug_span;
use tracing_result::Trace;

use crate::ReleaseDetails;

impl From<&Release> for ReleaseDetails {
    fn from(release: &Release) -> Self {
        let artist = release.main_artist().clone().unwrap_or_default();
        let title = release.title.clone();
        let album = [artist, title].join(": ").to_shared_string();
        let comment = release
            .disambiguation
            .as_ref()
            .filter(|comment| !comment.is_empty())
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
        let id = release.id.to_shared_string();
        Self {
            album,
            barcode,
            comment,
            date,
            location,
            id,
            ..Default::default()
        }
    }
}

impl TryFrom<&Disc> for ReleaseDetails {
    type Error = io::Error;

    fn try_from(disc: &Disc) -> Result<Self, Self::Error> {
        let _debug_span = debug_span!("tryfrom_disc_for_albumdetails").entered();
        let release = disc
            .release()
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no release found"))
            .or_warn("")?;
        let mut details = ReleaseDetails::from(release);
        let thumb = disc
            .get_thumbnail(&release.id)
            .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no thumbnail found"))
            .or_warn("")?;
        let image = Image::load_from_data(&thumb.data, Some("jpg"))
            .map_err(io::Error::other)
            .or_warn("")?;
        details.thumbnail = image;
        Ok(details)
    }
}

impl ReleaseDetails {
    pub fn for_disc(disc: &Disc) -> Option<ModelRc<Self>> {
        let _debug_span = debug_span!("AlbumDetails::for_disc").entered();
        let details: Vec<_> = disc
            .all_releases()?
            .iter()
            .filter_map(|&release| {
                let mut details = ReleaseDetails::from(release);
                try bikeshed io::Result<_> {
                    let thumb = disc
                        .get_thumbnail(&release.id)
                        .ok_or_else(|| io::Error::new(ErrorKind::NotFound, "no thumbnail found"))
                        .or_warn("")?;
                    let image = Image::load_from_data(&thumb.data, Some("jpg"))
                        .map_err(io::Error::other)
                        .or_warn("")?;
                    details.thumbnail = image;
                }
                .ok()?;
                Some(details)
            })
            .collect();
        Some(ModelRc::from(details.as_slice()))
    }
}

#[cfg(test)]
mod tests {

    use std::fs;

    use metaflac::block::{Picture, PictureType};
    use redbook::{
        Disc,
        tagging::PictureExt,
        test_fixtures::albums::TestAlbum::{self, *},
    };
    use slint::{Image, Model};

    use super::*;

    #[test]
    fn albumdetails_from_release() {
        let album: TestAlbum = DefinitelyMaybe;

        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks, leadout).unwrap();
        disc.set_musicbrainz(musicbrainz);
        disc.set_release_index(Some(album.release()));

        let details = ReleaseDetails::from(disc.release().unwrap());

        let expected = ReleaseDetails {
            album: "Oasis: Definitely Maybe".into(),
            date: "1994-08-30".into(),
            location: "GB".into(),
            barcode: "5017556601693".into(),
            comment: "(Plant MFG pressing)".into(),
            id: "9822581d-98bf-3f97-a94c-4b1350d090aa".into(),
            ..Default::default()
        };

        assert_eq!(details.album, expected.album);
        assert_eq!(details.date, expected.date);
        assert_eq!(details.location, expected.location);
        assert_eq!(details.barcode, expected.barcode);
        assert_eq!(details.comment, expected.comment);
    }

    #[test]
    fn albumdetails_from_disc() {
        let album: TestAlbum = DefinitelyMaybe;

        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks, leadout).unwrap();
        disc.set_musicbrainz(musicbrainz);
        disc.set_release_index(Some(album.release()));
        let release_id = disc.release().unwrap().id.clone();
        let thumb = fs::read(album.thumbnail_path()).unwrap();
        let image = Picture::from_jpeg(PictureType::CoverFront, "Front Cover", thumb);
        disc.add_thumbnail(release_id, image);

        let details = ReleaseDetails::try_from(&disc).unwrap();

        let expected = ReleaseDetails {
            album: "Oasis: Definitely Maybe".into(),
            date: "1994-08-30".into(),
            location: "GB".into(),
            barcode: "5017556601693".into(),
            comment: "(Plant MFG pressing)".into(),
            thumbnail: Image::load_from_path(&album.thumbnail_path()).unwrap(),
            id: "9822581d-98bf-3f97-a94c-4b1350d090aa".into(),
        };

        assert_eq!(details.album, expected.album);
        assert_eq!(details.date, expected.date);
        assert_eq!(details.location, expected.location);
        assert_eq!(details.barcode, expected.barcode);
        assert_eq!(details.comment, expected.comment);
        assert_eq!(
            details.thumbnail.to_rgb8().unwrap().as_slice(),
            expected.thumbnail.to_rgb8().unwrap().as_slice()
        );
    }

    #[test]
    fn all_details_for_disc() {
        let album: TestAlbum = DefinitelyMaybe;

        let toc = album.expected_toc();
        let tracks = album.expected_tracks_minimal();
        let leadout = album.expected_leadout();
        let musicbrainz = album.expected_musicbrainz();

        let mut disc = Disc::new(toc, tracks, leadout).unwrap();
        disc.set_musicbrainz(musicbrainz);

        let releases: Vec<_> = disc
            .all_releases()
            .unwrap()
            .iter()
            .map(|release| release.id.clone())
            .collect();

        for (release_id, image) in album.expected_thumbnails() {
            disc.add_thumbnail(release_id, image);
        }

        let albums = ReleaseDetails::for_disc(&disc).unwrap();
        assert_eq!(albums.row_count(), releases.len());
        for details in albums.iter() {
            assert!(releases.contains(&details.id.to_string()));
            let release = *disc
                .all_releases()
                .unwrap()
                .iter()
                .find(|release| *release.id == *details.id)
                .unwrap();
            let album = [release.main_artist().unwrap(), release.title.clone()].join(": ");
            assert_eq!(details.album, album);
            assert_eq!(
                details.location,
                release.country.clone().unwrap_or_default()
            );
            assert_eq!(details.barcode, release.barcode.clone().unwrap_or_default());
        }
    }
}
