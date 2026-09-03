#![allow(stable_features)]
#![feature(never_type)]
#![feature(try_blocks)]
#![feature(try_trait_v2)]
#![feature(try_trait_v2_residual)]

use std::{
    convert::Infallible,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    process::Termination as _T,
    str::FromStr,
    sync::mpsc,
    thread,
};

use clap::Parser;
use exit_safely::Termination;
use metaflac::{
    Block, Tag,
    block::{Picture, PictureType},
};
use redbook::{AudioCd, AudioCdExt, AudioCdExtMut, RippedTrack, tagging::PictureExt};
use try_v2::Try;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
enum SelectedTrack {
    All,
    One(usize),
}

mod _releases;
mod _tracing;
mod cli;
mod sanitize;
pub(crate) use cli::Rip;

use crate::_releases::release_menu;
use crate::sanitize::FilenameSanitize;

#[cfg(target_family = "windows")]
fn main() -> Exit<()> {
    use tracing::field::Empty;

    let ripper = Rip::try_parse()?;

    let drive = PathBuf::from_str(&ripper.drive)?;

    ripper.init_tracing()?;
    let mut _info = tracing::info_span!("Rip", drive = %drive.display(), title = Empty).entered();

    let mut cd: AudioCd = AudioCd::new(drive)?;

    let _ = cd.disc_mut().update_musicbrainz();
    match (cd.disc().release(), ripper.non_interactive) {
        (Some(_), _) => (),
        (None, true) => {
            let latest_release = cd
                .disc()
                .musicbrainz()
                .and_then(|disc| disc.releases.as_ref())
                .as_ref()
                .and_then(|releases| {
                    releases
                        .iter()
                        .max_by_key(|release| {
                            release
                                .date
                                .as_ref()
                                .map(|date| date.into_naive_date(1, 1, 1).ok())
                        })
                        .and_then(|latest_release| {
                            releases
                                .iter()
                                .position(|release| release.id == latest_release.id)
                        })
                });
            cd.disc_mut().set_release(latest_release);
            tracing::info!(
                name: "selected latest release",
                title = %cd.disc().title().unwrap_or_default(),
                country = %cd.disc().release().unwrap().country.clone().unwrap_or_default(),
                date = %cd.disc().release().unwrap().date.as_ref().cloned().unwrap_or_default()
            );
        }
        (None, false) => {
            try {
                let release_menu = release_menu(cd.disc())?;
                println!("{}", release_menu.table);
                let selected = loop {
                    #[expect(unused, reason = "loop on error")]
                    try {
                        let mut input = String::new();
                        println!("\nEnter the number of the release to use:");

                        io::stdin().read_line(&mut input).map_err(|error| {
                            println!(
                                "oops ... problem understanding you ... it's me, not you. {error}"
                            );
                        })?;

                        let choice = input.trim().parse::<usize>().map_err(|error| {
                            println!("oops ... try again {input} is not a number");
                        })?;

                        let index = release_menu.index_for(choice).ok_or_else(|| {
                            println!("oops ... I can't find release number {choice}");
                        })?;

                        break index;
                    };
                };
                cd.disc_mut().set_release(Some(selected));
            };
        }
    };

    let mut disc_title = cd.disc().title().unwrap_or_else(|| "Unknown".to_string());

    _info.record("title", &disc_title);

    if cd
        .disc()
        .release()
        .and_then(|release| release.media.as_ref().map(|all_media| all_media.len()))
        .unwrap_or_default()
        > 1
    {
        disc_title.push_str(&format!(
            " [Disc {}]",
            cd.disc()
                .disc_number()
                .unwrap_or_else(|| "Unknown".to_string())
        ));
    }

    let selected_track = match (ripper.all, ripper.track_number) {
        (true, Some(_)) => {
            return Exit::InvocationError(
                "Cannot specify both --all and a track number".to_string(),
            );
        }
        (true, None) => SelectedTrack::All,
        (false, Some(n)) => SelectedTrack::One(n),
        (false, None) => {
            println!("\nAvailable tracks:");

            for track in cd.disc().tracks() {
                let track_name = track.title().unwrap_or_else(|| "Unknown".to_string());
                println!("{n}. {track_name}", n = track.toc_entry.track);
            }
            println!("a. All tracks");

            loop {
                #[expect(unused, reason = "loop on error")]
                try {
                    let mut input = String::new();
                    println!("\nEnter the track number to rip (a for all):");

                    let _ = io::stdin().read_line(&mut input).map_err(|error| {
                        println!(
                            "oops ... problem understanding you ... it's me, not you. {error}"
                        );
                    })?;

                    let input_trimmed = input.trim().to_lowercase();
                    if input_trimmed == "a" {
                        break SelectedTrack::All;
                    }

                    let choice: usize = input_trimmed.parse().map_err(|error| {
                        println!("oops ... try again {input_trimmed} is not a number");
                    })?;

                    let valid = cd
                        .disc()
                        .tracks()
                        .any(|t| t.toc_entry.track as usize == choice);
                    valid.ok_or_else(|| {
                        println!("oops ... I can't find track number {choice}");
                    })?;

                    break SelectedTrack::One(choice);
                };
            }
        }
    };

    let track_numbers = match selected_track {
        SelectedTrack::All => 1..=cd.disc().tracks().len(),
        SelectedTrack::One(n) => n..=n,
    };

    let artist = cd
        .disc()
        .main_artist()
        .unwrap_or_else(|| "Unknown".to_string());

    // TODO: #24 handle invlaid chars in filenames: see https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#naming-conventions
    let output_dir = PathBuf::from(artist.sanitize_filename()).join(disc_title.sanitize_filename());
    dbg!(&output_dir);
    fs::create_dir_all(&output_dir)?;

    let _ = cd.disc_mut().update_cover_art();
    if let Some(Err(error_saving_coverart)) = cd.disc().save_cover_art(&output_dir) {
        dbg!(error_saving_coverart);
    };

    let cd = cd.lock();
    let disc = cd.disc().clone();

    let (ripped_tracks_tx, ripped_tracks_rx) = mpsc::channel::<RippedTrack>();

    let ripper = thread::spawn(move || {
        for track_number in track_numbers.clone() {
            try {
                use humanize_duration::{Truncate, prelude::DurationExt};

                let track = cd.disc().track(track_number).unwrap();
                let track_name = track.title().unwrap_or_default();

                const SPAN_TARGET: &str = "rip track";
                let _warn = tracing::warn_span!(SPAN_TARGET, track_number).entered();
                let _info = tracing::info_span!(SPAN_TARGET, track_name).entered();

                tracing::info!(target: SPAN_TARGET, "rip_track_start");

                let start = std::time::Instant::now();
                let ripped = cd.rip(track_number).ok()?;
                let duration = start.elapsed().human(Truncate::Millis).to_string();

                tracing::info!(target: SPAN_TARGET, duration, "rip_track_done");

                ripped_tracks_tx.send(ripped).ok()?;
            };
        }
    });

    let encoder = thread::spawn(move || {
        let enc = try {
            while let Ok(ripped) = ripped_tracks_rx.recv() {
                let track_number = ripped.track_number;
                let track = disc.track(track_number).unwrap();
                let track_name = track.title().unwrap_or_default();

                tracing::debug!(
                    target: "encode",
                    track = track_number,
                    name = %track_name,
                    "encode_start"
                );
                let start = std::time::Instant::now();

                let flac_path = output_dir.join(track.filename()).with_extension("flac");
                let flac = ripped.to_flac();
                let mut flac_file = File::create_new(&flac_path)?;
                flac_file.write_all(flac.as_slice())?;
                println!(
                    "Track {} ripped to {}",
                    track.track_number(),
                    flac_path.display()
                );
                let bytes_written = flac.as_slice().len();

                let mut tag = Tag::read_from_path(&flac_path).unwrap();
                if let Some(tags) = disc.tag_for(track_number) {
                    let vorbis = tag.vorbis_comments_mut();
                    vorbis.comments.extend(tags.comments);
                }

                if let Some(cover) =
                    disc.cover_art()
                        .cloned()
                        .or(fs::read(output_dir.join("front.jpeg")).ok().map(|data| {
                            Picture::from_jpeg(PictureType::CoverFront, "Front Cover", data)
                        }))
                {
                    tag.push_block(Block::Picture(cover));
                }

                tag.write_to_path(&flac_path).unwrap();

                let duration = start.elapsed();
                tracing::debug!(
                    target: "encode",
                    track = track_number,
                    bytes = bytes_written,
                    duration_secs = ?duration.as_secs_f64(),
                    "encode_done"
                );
            }
        };
        drop(ripped_tracks_rx);
        enc
    });

    ripper
        .join()
        .map_err(|panicked| Exit::Error(format!("ripping panicked: {panicked:?}")))?;
    encoder
        .join()
        .map_err(|panicked| Exit::Error(format!("encoding panicked: {panicked:?}")))??;

    Exit::Ok(())
}

#[cfg(not(target_family = "windows"))]
fn main() -> Exit<()> {
    Exit::Ok(())
}

#[derive(Debug, Termination, Try, PartialEq, PartialOrd, Eq, Ord)]
#[FromResidual(Result<_, Self::Residual>)]
#[repr(u8)]
#[must_use]
pub enum Exit<T: _T> {
    Ok(T) = 0,
    Error(String) = 1,
    InvocationError(String) = 2,
    IO(String) = 3,
    Logging(String) = 4,
}

impl<T: _T> From<clap::Error> for Exit<T> {
    fn from(e: clap::Error) -> Self {
        Self::InvocationError(e.to_string())
    }
}

impl<T: _T> From<io::Error> for Exit<T> {
    fn from(e: io::Error) -> Self {
        Self::IO(e.to_string())
    }
}

impl<T: _T> From<Infallible> for Exit<T> {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}
