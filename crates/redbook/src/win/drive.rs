//! Handles direct hardware access via Windows APIs

use std::io;

use thread_safely::Context;
use tracing::field::Empty;
use tracing_result::Trace;

use super::{convert::Guid, sys::GUID_DEVINTERFACE_CDROM};
#[cfg(test)]
use crate::test_fixtures::albums::TestAlbum;
use crate::{
    Device, RipProgress,
    drive::CdDrive,
    win::sys::{DeviceInfoSet, DriveHandle},
};

/// Iterator over all the available drives, which have an AudioCd present
pub struct CdDrives {
    deviceinfoset: DeviceInfoSetIter,
    /// Thread context for cancellation & status reporting
    cx: Context<RipProgress>,
}

impl Iterator for CdDrives {
    type Item = CdDrive<DriveHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let device = self.deviceinfoset.next()?;
        let path = device
            .fs_path()
            .or_error("getting path for next device")
            .ok()?;
        let tocentries = device
            .read_toc()
            .or_error("reading TOC for next device")
            .ok()?;
        let cx = self.cx.clone();

        Some(CdDrive::new(path, device, tocentries, cx))
    }
}

impl IntoIterator for DeviceInfoSet {
    type Item = DriveHandle;

    type IntoIter = DeviceInfoSetIter;

    fn into_iter(self) -> Self::IntoIter {
        DeviceInfoSetIter { infoset: self, .. }
    }
}

/// TODO: Docs
pub struct DeviceInfoSetIter {
    infoset: DeviceInfoSet,
    current_index: usize = 0,
}

impl Iterator for DeviceInfoSetIter {
    type Item = DriveHandle;

    fn next(&mut self) -> Option<Self::Item> {
        let drive_index = self.current_index;
        self.current_index += 1;

        let guid = self.infoset.class();

        let debug = tracing::debug_span!(
            "DeviceInfoSetIter::next",
            drive_index,
            %guid,
        )
        .entered();

        let drive_data = self
            .infoset
            .get_member(drive_index.strict_cast())
            .or_warn("error getting drive data")
            .ok()
            .flatten()?;

        match DriveHandle::open(drive_data.path()) {
            Ok(drive) => Some(drive),
            Err(error) if error.raw_os_error() == Some(21) => {
                // OS error 21 (device not ready) = no disc in drive
                drop(debug);
                self.next()
            }
            Err(error) => {
                tracing::error!(%error, "opening device");
                None
            }
        }
    }
}

/// Get all the available drives, which have an AudioCd present
pub fn all_drives(cx: Context<RipProgress>) -> io::Result<CdDrives> {
    let debug = tracing::debug_span!("all_drives", handle = Empty).entered();

    let deviceinfoset = DeviceInfoSet::for_device_class(Guid(GUID_DEVINTERFACE_CDROM))?;
    debug.record("handle", format!("{deviceinfoset:?}"));

    tracing::debug!("got device infoset");
    Ok(CdDrives {
        deviceinfoset: deviceinfoset.into_iter(),
        cx,
        ..
    })
}

#[cfg(test)]
impl TryFrom<&CdDrive<DriveHandle>> for TestAlbum {
    type Error = io::Error;

    fn try_from(drive: &CdDrive<DriveHandle>) -> Result<Self, Self::Error> {
        let handle = drive.device();
        handle.try_into()
    }
}

#[cfg(test)]
mod miri {
    use cdtoc::Toc;

    use crate::{Drive, test_fixtures::albums::TestAlbum::*};

    use super::*;

    #[test]
    fn all() {
        let drives = all_drives(Default::default()).unwrap();
        let albums: Vec<_> = drives
            .map(|drive| Toc::try_from(drive.toc()).unwrap())
            .collect();
        let expected = [
            DefinitelyMaybe.expected_toc(),
            TheWallDisc1.expected_toc(),
            TheWallDisc2.expected_toc(),
        ];
        assert_eq!(albums, expected);
    }
}
