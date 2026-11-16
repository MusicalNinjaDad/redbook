#[cfg(target_family = "windows")]
use std::path::PathBuf;

#[cfg(target_family = "windows")]
use itertools::Itertools;
#[cfg(target_family = "windows")]
use redbook::{
    AudioCd,
    drive::{CdDrives, DriveError},
};

#[cfg(target_family = "windows")]
fn main() -> redbook::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let drive = args.get(1).map(PathBuf::from);
    let cd = match drive {
        Some(drive) => AudioCd::open(&drive, Default::default())?,
        None => {
            let drive = CdDrives::all(Default::default())?
                .next()
                .ok_or(DriveError::NoDrivesFound)??;
            AudioCd::try_from(drive)?
        }
    };
    dbg!(&cd);
    // spec here: https://github.com/metabrainz/libdiscid/blob/master/src/disc.c#L427
    let mbtoc_prefix = format_args!(
        "{first}+{last}+{leadout}",
        first = cd
            .toc_entries()
            .audio_tracks()
            .next()
            .unwrap()
            .trackno()
            .cast::<u8>(),
        last = cd
            .toc_entries()
            .audio_tracks()
            .last()
            .unwrap()
            .trackno()
            .cast::<u8>(),
        leadout = cd.toc().leadout()
    );
    let mbtoc = cd
        .toc_entries()
        .audio_tracks()
        .map(|t| t.start().cast::<u32>())
        .join("+");
    let submission_url = format!(
        "https://musicbrainz.org/cdtoc/attach?id={discid}&tracks={tracks}&toc={mbtoc_prefix}+{mbtoc}",
        discid = cd.toc().musicbrainz_id(),
        tracks = cd.toc_entries().audio_tracks().count(),
    );
    dbg!(submission_url);
    Ok(())
}

#[cfg(not(target_family = "windows"))]
fn main() {}
