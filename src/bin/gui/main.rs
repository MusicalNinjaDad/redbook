#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]

#[cfg(feature = "gui")]
mod album;

mod slint;
use slint::*;

use redbook::{
    Disc,
    test_fixtures::albums::TestAlbum::{self, *},
};

#[cfg(feature = "gui")]
fn main() {
    let album: TestAlbum = DefinitelyMaybe;

    let toc = album.expected_toc();
    let tracks = album.expected_tracks_minimal();
    let leadout = album.expected_leadout();
    let musicbrainz = album.expected_musicbrainz();

    let mut disc = Disc::new(toc, tracks, leadout).unwrap();
    disc.set_musicbrainz(musicbrainz);
    for (release_id, image) in album.expected_thumbnails() {
        disc.add_thumbnail(release_id, image);
    }

    let albums = AlbumDetails::for_disc(&disc).unwrap();

    let app = MainWindow::new().unwrap();
    app.set_albums(albums);
    app.run().unwrap()
}

#[cfg(not(feature = "gui"))]
fn main() {}
