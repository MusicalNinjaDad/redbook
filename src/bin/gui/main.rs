#![cfg_attr(unstable_integer_casts, feature(integer_casts))]
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
use redbook::{AudioCd, AudioCdExt, win::drive::all_drives};
#[cfg(feature = "gui")]
use slint::*;
#[cfg(feature = "gui")]
use std::{
    io,
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};

#[cfg(feature = "gui")]
fn main() -> io::Result<()> {
    output::init_tracing()?;
    let app = MainWindow::new().unwrap();

    let drive = all_drives()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
    let cd = AudioCd::try_from(drive)?;
    let cd = Arc::new(Mutex::from(cd));

    let (to_rip_tx, to_rip_rx) = mpsc::channel::<Vec<usize>>();

    let app_ = app.as_weak();
    let cd_ = cd.clone();
    let setup = thread::spawn(move || {
        let cd = cd_;
        {
            let mut disc_lock = cd.lock().expect("TODO #68 tracing on poison");
            let disc = disc_lock.disc_mut();
            disc.update_musicbrainz()?;
            disc.update_thumbnails()?;
        }

        ::slint::invoke_from_event_loop(move || {
            let app = app_.clone().unwrap();
            let releases = {
                let disc_lock = cd
                    .lock()
                    .expect("TODO #69 don't block event loop waiting for lock");
                let disc = disc_lock.disc();
                ReleaseDetails::for_disc(disc).unwrap()
            };
            app.set_releases(releases);
            app.on_select_release(select_release(app_.clone(), cd));
            app.on_rip(rip(app_, to_rip_tx));
        })
        .map_err(io::Error::other)
    });

    let ripper = thread::spawn(move || {
        while let Ok(tracks) = to_rip_rx.recv() {
            let disc_lock = cd
                .lock()
                .expect("TODO #68 error handling & tracing on poison");
            let disc = disc_lock.disc();
            for track_number in tracks {
                let track = disc.track(track_number);
                tracing::info!(ripping = ?track);
            }
        }
    });

    app.run().unwrap();

    setup.join().expect("TODO #71 panic handling")?;
    ripper.join().expect("TODO #71 panic handling");

    Ok(())
}

#[cfg(feature = "gui")]
fn select_release(app: Weak<MainWindow>, cd: Arc<Mutex<AudioCd>>) -> impl FnMut(ReleaseDetails) {
    move |release: ReleaseDetails| {
        let app = app.clone().unwrap();
        {
            let mut disc_lock = cd.lock().expect("TODO #68 error handling on posion");
            let disc = disc_lock.disc_mut();

            disc.set_release_by_id(Some(&release.id));

            let albums = [release];
            app.set_releases(ModelRc::from(albums.as_slice()));

            let tracks: Vec<TrackDetails> = disc.tracks().map(TrackDetails::from).collect();
            app.set_tracks(ModelRc::from(tracks.as_slice()));
        }
    }
}

#[cfg(feature = "gui")]
fn rip(app: Weak<MainWindow>, channel: Sender<Vec<usize>>) -> impl FnMut() {
    move || {
        let app = app.clone().unwrap();
        let tracks = app
            .get_tracks()
            .filter(|track| track.rip)
            .map(|track| track.number.strict_cast())
            .iter()
            .collect();
        channel
            .send(tracks)
            .expect("TODO #70 error handling on broken channel");
    }
}

#[cfg(not(feature = "gui"))]
fn main() {}
