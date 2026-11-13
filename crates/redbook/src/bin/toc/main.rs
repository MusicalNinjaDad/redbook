#[cfg(target_family = "windows")]
use std::path::PathBuf;

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
    Ok(())
}

#[cfg(not(target_family = "windows"))]
fn main() {}
