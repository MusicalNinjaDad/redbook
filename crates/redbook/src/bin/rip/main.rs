#![allow(stable_features)]
#![feature(never_type)]
#![feature(try_blocks)]
#![feature(try_trait_v2)]
#![feature(try_trait_v2_residual)]
#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]
#![cfg_attr(
    not(target_family = "windows"),
    expect(unused_imports, reason = "stubs")
)]

use std::{
    convert::Infallible,
    fs::{self, File},
    io::{self, Write},
    path::PathBuf,
    process::Termination as _T,
    sync::mpsc,
    thread,
};

use clap::Parser;
use exit_safely::Termination;
use humanize_duration::{Truncate, prelude::DurationExt};
use metaflac::{
    Block, Tag,
    block::{Picture, PictureType},
};
#[cfg(target_family = "windows")]
use redbook::{
    AudioCd, AudioCdExt, RippedTrack,
    tagging::{PictureExt, VorbisTagExt},
    win::drive::all_drives,
};
#[cfg(target_family = "windows")]
use tracing_result::Trace;
use try_v2::Try;

#[derive(Debug, Clone, Copy)]
#[cfg_attr(not(target_family = "windows"), expect(dead_code, reason = "stubs"))]
enum SelectedTrack {
    All,
    One(usize),
}

mod cli;
mod output;
mod release_menu;
mod sanitize;
pub(crate) use cli::Rip;

use crate::release_menu::release_menu;
use crate::sanitize::FilenameSanitize;

#[cfg(target_family = "windows")]
fn main() -> Exit<()> {
    use tracing::field::Empty;

    let ripper = Rip::try_parse()?;

    ripper.init_tracing()?;
    let info = tracing::info_span!("Rip", drive = Empty, title = Empty, artist = Empty).entered();

    let mut cd = match ripper.drive {
        Some(drive) => {
            info.record("drive", drive.display().to_string());
            AudioCd::new(drive)?
        }
        None => {
            let drive = all_drives()?
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
            AudioCd::try_from(drive)?
        }
    };

    #[expect(unused_must_use, reason = "do not abort if musicbrainz not available")]
    cd.disc_mut().update_musicbrainz();
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
            cd.disc_mut().set_release_index(latest_release);
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
                    #[expect(unused_must_use, reason = "loop on error")]
                    try {
                        let mut input = String::new();
                        println!("\nEnter the number of the release to use:");

                        io::stdin().read_line(&mut input).map_err(|error| {
                            println!(
                                "oops ... problem understanding you ... it's me, not you. {error}"
                            );
                        })?;

                        let choice = input.trim().parse::<usize>().map_err(|_| {
                            println!("oops ... try again {input} is not a number");
                        })?;

                        let &release = release_menu.releases.get(choice - 1).ok_or_else(|| {
                            println!("oops ... I can't find release number {choice}");
                        })?;

                        break release.clone();
                    };
                };
                cd.disc_mut().set_release(Some(&selected));
                tracing::debug!(
                    name: "manually selected release",
                    title = %cd.disc().title().unwrap_or_default(),
                    country = %cd.disc().release().unwrap().country.clone().unwrap_or_default(),
                    date = %cd.disc().release().unwrap().date.as_ref().cloned().unwrap_or_default()
                );
            };
        }
    };

    let mut disc_title = cd.disc().title().unwrap_or_else(|| "Unknown".to_string());
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
    info.record("title", &disc_title);

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
                #[expect(unused_must_use, reason = "loop on error")]
                try {
                    let mut input = String::new();
                    println!("\nEnter the track number to rip (a for all):");

                    let _: usize = io::stdin().read_line(&mut input).map_err(|error| {
                        println!(
                            "oops ... problem understanding you ... it's me, not you. {error}"
                        );
                    })?;

                    let input_trimmed = input.trim().to_lowercase();
                    if input_trimmed == "a" {
                        break SelectedTrack::All;
                    }

                    let choice: usize = input_trimmed.parse().map_err(|_| {
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
    info.record("artist", &artist);

    // TODO: #24 handle invlaid chars in filenames: see https://learn.microsoft.com/en-us/windows/win32/fileio/naming-a-file#naming-conventions
    let output_dir = PathBuf::from(artist.sanitize_filename()).join(disc_title.sanitize_filename());
    tracing::debug!(output_dir = %output_dir.display());
    fs::create_dir_all(&output_dir)?;

    #[expect(
        unused_must_use,
        reason = "do not abort if CoverArtArchive not available"
    )]
    cd.disc_mut().update_cover_art();
    #[expect(unused_must_use, reason = "don't abort if unable to save cover art")]
    cd.disc().save_cover_art(&output_dir);

    let (ripped_tracks_tx, ripped_tracks_rx) = mpsc::channel::<RippedTrack>();

    let ripper = thread::spawn(move || {
        for track_number in track_numbers.clone() {
            try {
                let track = cd.disc().track(track_number).unwrap();
                let track_name = track.title().unwrap_or_default();

                const SPAN_TARGET: &str = "rip track";
                let _warn = tracing::warn_span!(SPAN_TARGET, track_number).entered();
                let _info = tracing::info_span!(SPAN_TARGET, track_name).entered();

                tracing::info!(target: SPAN_TARGET, "rip_track_start");

                let start = std::time::Instant::now();
                let ripped =
                    try bikeshed io::Result<_> { cd.rip(track_number).or_warn("")? }.ok()?;
                let duration = start.elapsed().human(Truncate::Millis).to_string();

                tracing::info!(target: SPAN_TARGET, duration, "rip_track_done");

                ripped_tracks_tx.send(ripped).ok()?;
            };
        }
    });

    let encoder = thread::spawn(move || {
        while let Ok(ripped) = ripped_tracks_rx.recv() {
            #[expect(unused_must_use, reason = "lopp on error")]
            #[expect(
                clippy::unnecessary_operation,
                reason = "clippy error - need to raise issue linking to bikeshed tracking issue"
            )]
            try bikeshed io::Result<_> {
                let tag = &ripped.tags;
                let track_number = tag.track().unwrap_or_default();
                let track_name = tag.full_title();
                let _debug_span =
                    tracing::debug_span!("encode", track = track_number, name = %track_name)
                        .entered();
                tracing::debug!("encode_start");

                let start = std::time::Instant::now();

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

    ripper
        .join()
        .map_err(|panicked| Exit::Error(format!("ripping panicked: {panicked:?}")))?;
    encoder
        .join()
        .map_err(|panicked| Exit::Error(format!("encoding panicked: {panicked:?}")))?;

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
