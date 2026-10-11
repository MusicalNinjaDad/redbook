#[cfg(target_family = "windows")]
use std::io;
#[cfg(target_family = "windows")]
use std::path::PathBuf;

#[cfg(target_family = "windows")]
use redbook::{AudioCd, AudioCdExt};
#[cfg(target_family = "windows")]
use redbook::hex::hex_dump;
#[cfg(target_family = "windows")]
use redbook::win::drive::{self, CdDrive, all_drives};

#[cfg(target_family = "windows")]
fn main() -> io::Result<()>{
    let args: Vec<_> = std::env::args().collect();
    let drive = args.get(1).map(PathBuf::from);
    let mut cd = match drive {
        Some(drive) => {
            dbg!(&drive);
            AudioCd::new(drive)?
        }
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
