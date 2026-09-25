#![cfg_attr(unstable_integer_casts, feature(integer_casts))]
#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]
#![cfg_attr(unstable_try_blocks, feature(try_blocks))]

mod album;

mod output;

mod slint;

use ::slint::{Model, ModelRc, Weak};

use metaflac::{
    Block, Tag,
    block::{Picture, PictureType},
};

use redbook::{
    AudioCd, AudioCdExt, RipProgress, RippedTrack,
    tagging::{PictureExt, VorbisTagExt},
    win::drive::all_drives,
};

use slint::*;
use thread_safely::Controller;

use std::{
    fs::{self, File},
    io::{self, Write},
    sync::{
        Arc, Mutex,
        mpsc::{self, Sender},
    },
    thread,
};

use tracing_result::Trace;

fn main() -> io::Result<()> {
    output::init_tracing()?;
    let app = MainWindow::new().unwrap();
    let (rip_controller, rip_context) = Controller::<RipProgress>::new();
    let (enc_controller, enc_context) = Controller::<u32>::new();

    let drive = all_drives()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
    let mut cd = AudioCd::try_from(drive)?;
    cd.add_context(rip_context);
    let cd = Arc::new(Mutex::from(cd));

    let (to_rip_tx, to_rip_rx) = mpsc::channel::<Vec<usize>>();
    let (ripped_tx, ripped_rx) = mpsc::channel::<RippedTrack>();

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

    let ripper = thread::spawn(move || try {
        while let Ok(tracks) = to_rip_rx.recv() {
            let cd_lock = cd
                .lock()
                .expect("TODO #68 error handling & tracing on poison");
            let disc = cd_lock.disc();
            for track_number in tracks {
                let track = disc.track(track_number);
                tracing::info!(ripping = ?track);
                let ripped = cd_lock.rip(track_number).or_error("ripping")?;
                ripped_tx
                    .send(ripped)
                    .map_err(io::Error::other)
                    .or_error("sending")?;
            }
        }
    });

    let encoder = thread::spawn(move || {
        while let Ok(ripped) = ripped_rx.recv() {
            #[expect(unused_must_use, reason = "loop on error")]
            #[expect(
                clippy::unnecessary_operation,
                reason = "clippy error - need to raise issue linking to bikeshed tracking issue"
            )]
            try bikeshed io::Result<_> {
                enc_context.cancelled()?;
                enc_context
                    .reply(ripped.tags.track().unwrap_or_default())
                    .map_err(io::Error::other)
                    .or_warn("providing encoding status update");
                let tag = &ripped.tags;
                let track_number = tag.track().unwrap_or_default();
                let track_name = tag.full_title();
                let _debug_span =
                    tracing::debug_span!("encode", track = track_number, name = %track_name)
                        .entered();
                tracing::debug!("encode_start");

                let start = std::time::Instant::now();

                let output_dir = tag.directory();
                tracing::debug!(output_dir = %output_dir.display());
                fs::create_dir_all(&output_dir)?;
                let flac_path = output_dir.join(tag.filename()).with_extension("flac");
                let mut flac_file = File::create_new(&flac_path).or_warn("creating flac file")?;

                let flac = ripped.to_flac();
                flac_file
                    .write_all(flac.as_slice())
                    .or_warn("writing flac file")?;

                tracing::info!(
                    "File saved: {path} ({size} bytes)",
                    path = flac_path.display(),
                    size = flac.as_slice().len()
                );

                let RippedTrack { tags, coverart, .. } = ripped;
                let mut file_tag = Tag::read_from_path(&flac_path)
                    // TODO #67 error handling reading empty tag from file during encoding
                    .unwrap();
                file_tag
                    .vorbis_comments_mut()
                    .comments
                    .extend(tags.comments);

                if let Some(cover) = coverart.or(fs::read(output_dir.join("front.jpeg"))
                    .ok()
                    .map(|data| Picture::from_jpeg(PictureType::CoverFront, "Front Cover", data)))
                {
                    file_tag.push_block(Block::Picture(cover));
                }

                file_tag.write_to_path(&flac_path).unwrap();

                let duration = start.elapsed();
                tracing::debug!(
                    duration_secs = ?duration.as_secs_f64(),
                    "encode_done"
                );
            };
        }
    });

    app.run().unwrap();
    drop(app);
    rip_controller.cancel();
    enc_controller.cancel();
    tracing::debug!("app dropped, expecting threads to close now ...");

    setup.join().expect("TODO #71 panic handling")?;
    tracing::debug!("setup closed");

    ripper.join().expect("TODO #71 panic handling")?;
    tracing::debug!("ripper closed");

    encoder.join().expect("TODO #71 panic handling");
    tracing::debug!("encoder closed");

    Ok(())
}

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
