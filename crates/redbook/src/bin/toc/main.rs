#[cfg(target_family = "windows")]
use std::{io, path::PathBuf};

#[cfg(target_family = "windows")]
use redbook::{AudioCd, win::drive::all_drives};

#[cfg(target_family = "windows")]
fn main() -> io::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    let drive = args.get(1).map(PathBuf::from);
    let cd = match drive {
        Some(drive) => AudioCd::open(&drive, Default::default())?,
        None => {
            let drive = all_drives()?
                .next()
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no CD found"))?;
            AudioCd::try_from(drive)?
        }
    };
    dbg!(&cd);
    Ok(())
}

#[cfg(not(target_family = "windows"))]
fn main() {}
