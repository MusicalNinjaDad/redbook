#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]
#[cfg(feature = "gui")]
mod album;

#[cfg(feature = "gui")]
mod output;
#[cfg(feature = "gui")]
mod slint;
#[cfg(feature = "gui")]
use ::slint::{Model, ModelRc, Weak};
#[cfg(feature = "gui")]
use redbook::{AudioCd, AudioCdExt, Disc, win::drive::all_drives};
#[cfg(feature = "gui")]
use slint::*;
#[cfg(feature = "gui")]
use std::io;

#[cfg(feature = "gui")]
fn main() -> io::Result<()> {
    output::init_tracing()?;
    let app = MainWindow::new().unwrap();

    let drive = all_drives()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
    let mut cd = AudioCd::try_from(drive)?;

    let app_ = app.as_weak();
    let _worker_thread: std::thread::JoinHandle<io::Result<()>> = std::thread::spawn(move || {
        {
            let disc = cd.disc_mut();
            disc.update_musicbrainz()?;
            disc.update_thumbnails()?;
        }

        let disc = cd.disc().clone();
        ::slint::invoke_from_event_loop(move || {
            let app = app_.clone().unwrap();
            let releases = ReleaseDetails::for_disc(&disc).unwrap();
            app.set_releases(releases);
            app.on_select_release(select_release(app_, disc));
        })
        .map_err(io::Error::other)
    });

    app.run().unwrap();
    Ok(())
}

#[cfg(feature = "gui")]
fn select_release(app: Weak<MainWindow>, mut disc: Disc) -> impl FnMut(ReleaseDetails) {
    move |release: ReleaseDetails| {
        let app = app.clone().unwrap();
        disc.set_release_by_id(Some(&release.id));

        let albums = [release];
        app.set_releases(ModelRc::from(albums.as_slice()));

        let tracks: Vec<TrackDetails> = disc.tracks().map(TrackDetails::from).collect();
        app.set_tracks(ModelRc::from(tracks.as_slice()));

        let app_ = app.as_weak();
        app.on_rip(rip(app_));
    }
}

#[cfg(feature = "gui")]
fn rip(app: Weak<MainWindow>) -> impl FnMut() {
    move || {
        let app = app.clone().unwrap();
        let tracks = app.get_tracks();
        for track in tracks.iter().filter(|track| track.rip) {
            tracing::info!(ripping = ?track.title);
        }
    }
}

#[cfg(not(feature = "gui"))]
fn main() {}
