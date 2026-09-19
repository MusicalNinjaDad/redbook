#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]
#[cfg(feature = "gui")]
mod album;

mod output;
mod slint;

use std::io;

use redbook::{AudioCd, AudioCdExt, AudioCdExtMut, win::drive::all_drives};

use slint::*;

use ::slint::{Model, ModelRc, Weak};

#[cfg(feature = "gui")]
fn main() -> io::Result<()> {
    output::init_tracing()?;
    let app = MainWindow::new().unwrap();

    let drive = all_drives()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
    let mut cd = AudioCd::try_from(drive)?;
    let disc = cd.disc_mut();

    let app_ = app.as_weak();
    let update_musicbrainz = std::thread::spawn(move || {
        disc.update_musicbrainz()?;
        disc.update_thumbnails()?;

        let albums = ReleaseDetails::for_disc(disc).unwrap();
        Ok(())
    });

    let app_ = app.as_weak();
    app.on_select_release(select_release(app_, cd));

    app.run().unwrap();
    Ok(())
}

fn select_release(app: Weak<MainWindow>, mut cd: AudioCd) -> impl FnMut(ReleaseDetails) {
    move |release: ReleaseDetails| {
        let app = app.upgrade().unwrap();
        cd.disc_mut().set_release_by_id(Some(&release.id));

        let albums = [release];
        app.set_releases(ModelRc::from(albums.as_slice()));

        let tracks: Vec<TrackDetails> = cd.disc().tracks().map(TrackDetails::from).collect();
        app.set_tracks(ModelRc::from(tracks.as_slice()));

        let app_ = app.as_weak();
        app.on_rip(rip(app_));
    }
}

fn rip(app: Weak<MainWindow>) -> impl FnMut() {
    move || {
        let app = app.upgrade().unwrap();
        let tracks = app.get_tracks();
        for track in tracks.iter().filter(|track| track.rip) {
            tracing::info!(ripping = ?track.title);
        }
    }
}

#[cfg(not(feature = "gui"))]
fn main() {}
