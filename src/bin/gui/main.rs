#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]
#[cfg(feature = "gui")]
mod album;

mod output;
mod slint;

use std::io;

use redbook::{AudioCd, AudioCdExt, AudioCdExtMut, win::drive::all_drives};

use slint::*;

use ::slint::ModelRc;

#[cfg(feature = "gui")]
fn main() -> io::Result<()> {
    output::init_tracing()?;
    let drive = all_drives()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
    let mut cd = AudioCd::try_from(drive)?;

    cd.disc_mut().update_musicbrainz()?;
    cd.disc_mut().update_thumbnails()?;

    let disc = cd.disc();
    let albums = ReleaseDetails::for_disc(disc).unwrap();

    let app = MainWindow::new().unwrap();
    app.set_releases(albums);
    let app2 = app.as_weak();
    let select_release = move |release: ReleaseDetails| {
        let app = app2.upgrade().unwrap();
        cd.disc_mut().set_release_by_id(Some(&release.id));

        let albums = [release];
        app.set_releases(ModelRc::from(albums.as_slice()));

        let tracks: Vec<TrackDetails> = cd.disc().tracks().map(TrackDetails::from).collect();
        app.set_tracks(ModelRc::from(tracks.as_slice()));
    };

    app.on_select_release(select_release);
    app.run().unwrap();

    Ok(())
}

#[cfg(not(feature = "gui"))]
fn main() {}
