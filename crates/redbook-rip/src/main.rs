#![cfg_attr(unstable_integer_casts, feature(integer_casts))]
#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]
#![cfg_attr(unstable_try_blocks, feature(try_blocks))]

mod album;

mod output;

mod slint;

use ::slint::{Model, ModelRc, VecModel, Weak};
use crossbeam::{
    channel::{Sender, unbounded},
    select,
};

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
    rc::Rc,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use tracing_result::Trace;

#[derive(Debug, Clone)]
struct EncodingProgress {
    doing: Option<u32>,
    done: Vec<u32>,
}

fn main() -> io::Result<()> {
    output::init_tracing()?;
    let app = MainWindow::new().unwrap();
    let (setup_controller, setup_context) = Controller::<!>::new();
    let (rip_controller, rip_context) = Controller::<RipProgress>::new();
    let (enc_controller, enc_context) = Controller::<EncodingProgress>::new();
    let enc_context2 = enc_context.clone();

    let drive = all_drives()?
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
    let mut cd = AudioCd::try_from(drive)?;
    cd.add_context(rip_context.clone());
    let cd = Arc::new(Mutex::from(cd));

    let (to_rip_tx, to_rip_rx) = unbounded::<Vec<usize>>();
    let (ripped_tx, ripped_rx) = unbounded::<RippedTrack>();

    let app_ = app.as_weak();
    let cd_ = cd.clone();
    let setup = thread::spawn(move || {
        let cd = cd_;
        try bikeshed io::Result<()> {
            let mut disc_lock = cd.lock().expect("TODO #68 tracing on poison");
            let disc = disc_lock.disc_mut();
            setup_context.cancelled()?;
            disc.update_musicbrainz()?;
            disc.update_thumbnails(setup_context.clone())?;
        }?;

        setup_context.cancelled()?;
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

    let ripper = thread::spawn(move || try bikeshed io::Result<()> {
        loop {
            select! {
                recv(to_rip_rx) -> tracks => {
                    rip_tracks(cd.clone(), tracks.map_err(io::Error::other)?, ripped_tx.clone())?;
                }
                default(Duration::from_millis(100)) => {
                    rip_context.cancelled()?;
                }
            };
        }
    });

    let encoder = thread::spawn(move || {
        let mut progress = EncodingProgress {
            doing: None,
            done: Vec::new(),
        };
        // cancelling ripper drops ripped_tx, so we don't need to loop on a select here ...
        while let Ok(ripped) = ripped_rx.recv() {
            #[expect(unused_must_use, reason = "loop on error")]
            #[expect(
                clippy::unnecessary_operation,
                reason = "clippy error - need to raise issue linking to bikeshed tracking issue"
            )]
            try bikeshed io::Result<_> {
                enc_context.cancelled()?;
                progress.doing = Some(ripped.tags.track().unwrap_or_default());
                enc_context
                    .reply(progress.clone())
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
                progress.done.push(track_number);
            };
            progress.doing = None;
            enc_context
                .reply(progress.clone())
                .map_err(io::Error::other)
                .or_warn("providing encoding status update");
        }
    });

    let app_ = app.as_weak();
    let rip_rx = rip_controller.receiver();
    let enc_rx = enc_controller.receiver();
    let progress_updates = thread::spawn(move || try {
        loop {
            select! {
                recv(rip_rx) -> progress => update_rip_progress(app_.clone(), progress.unwrap()),
                recv(enc_rx) -> progress => update_encoding_progress(app_.clone(), progress.unwrap()),
                default(Duration::from_millis(100)) => {
                    enc_context2.cancelled()?;
                }
            };
        }
    });

    app.run().unwrap();
    drop(app);
    setup_controller.cancel();
    rip_controller.cancel();
    enc_controller.cancel();
    tracing::debug!("app dropped, expecting threads to close now ...");

    #[expect(unused_must_use, reason = "closing down, ensure we close all threads")]
    setup.join();
    tracing::debug!("setup closed");

    #[expect(unused_must_use, reason = "closing down, ensure we close all threads")]
    ripper.join();
    tracing::debug!("ripper closed");

    #[expect(unused_must_use, reason = "closing down, ensure we close all threads")]
    encoder.join();
    tracing::debug!("encoder closed");

    #[expect(unused_must_use, reason = "closing down, ensure we close all threads")]
    progress_updates.join();
    tracing::debug!("progress updates closed");

    Ok(())
}

type TracksModel = VecModel<TrackDetails>;

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
            app.set_tracks(ModelRc::from(Rc::new(TracksModel::from(tracks))));
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

fn rip_tracks(
    cd: Arc<Mutex<AudioCd>>,
    tracks: Vec<usize>,
    ripped_tracks: Sender<RippedTrack>,
) -> io::Result<()> {
    let cd_lock = cd
        .lock()
        .expect("TODO #68 error handling & tracing on poison");
    let disc = cd_lock.disc();
    for track_number in tracks {
        let track = disc.track(track_number);
        tracing::info!(ripping = ?track);
        let ripped = cd_lock.rip(track_number).or_error("ripping")?;
        ripped_tracks
            .send(ripped)
            .map_err(io::Error::other)
            .or_error("sending")?;
    }
    Ok(())
}

fn update_rip_progress(app: Weak<MainWindow>, progress: RipProgress) {
    let RipProgress {
        track_number,
        bytes_processed,
        total_bytes,
    } = progress;
    let progress = bytes_processed as f32 / total_bytes as f32;
    app.upgrade_in_event_loop(move |app| {
        let tracks = app.get_tracks();
        let mut track = tracks.row_data(track_number - 1).unwrap();
        assert_eq!(track_number as i32, track.number);
        track.rip_progress = progress;
        tracks.set_row_data(track_number - 1, track);
    })
    .unwrap();
}

fn update_encoding_progress(app: Weak<MainWindow>, progress: EncodingProgress) {
    let EncodingProgress { doing, done } = progress;
    app.upgrade_in_event_loop(move |app| {
        let tracks = app.get_tracks();
        for (row, track) in tracks.iter().enumerate() {
            if done.contains(&(track.number as u32)) {
                let mut track = track.clone();
                track.rip_progress = 1.0;
                track.encoding = false;
                tracks.set_row_data(row, track);
            } else if Some(track.number as u32) == doing {
                let mut track = track.clone();
                track.encoding = true;
                tracks.set_row_data(row, track);
            }
        }
    })
    .unwrap();
}
