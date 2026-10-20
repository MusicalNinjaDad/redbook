//! Handles direct hardware access via Windows APIs

use std::io;

use thread_safely::Context;
use tracing::field::Empty;
use tracing_result::Trace;

use super::{convert::Guid, sys::GUID_DEVINTERFACE_CDROM};
#[cfg(test)]
use crate::test_fixtures::albums::TestAlbum;
use crate::{
    CdDrive, Device, Drive, RipProgress,
    win::sys::{DeviceDetails, DeviceInfoSet, DriveHandle},
};

impl TryFrom<DeviceDetails> for CdDrive<DriveHandle> {
    type Error = io::Error;

    fn try_from(device: DeviceDetails) -> Result<Self, Self::Error> {
        let handle = DriveHandle::open(device.path()).or_error("")?;
        let toc = handle.read_toc()?;
        let path = handle.fs_path()?;
        Ok(Self::new(path, handle, toc, Default::default()))
    }
}

/// Iterator over all the available drives, which have an AudioCd present
pub struct CdDrives {
    /// # SAFETY:
    /// Must be a valid handle (pointer *mut c_void) to device information set.
    /// Can only be constructed via [`all_drives`] which ensures this is upheld.
    deviceinfoset: DeviceInfoSet,
    /// Thread context for cancellation & status reporting
    cx: Context<RipProgress>,
    /// Index of next element to retrieve from deviceinfoset.
    /// `u32` as this is what the ffi calls use.
    current_index: u32 = 0,
}

impl Iterator for CdDrives {
    type Item = CdDrive<DriveHandle>;

    fn next(&mut self) -> Option<Self::Item> {
        let drive_index = self.current_index;
        self.current_index += 1;

        let deviceinfoset = &self.deviceinfoset;

        let debug = tracing::debug_span!(
            "next drive",
            drive_index,
            guid = Empty,
            path_length = Empty,
            path = Empty,
        )
        .entered();

        let drive_data = deviceinfoset
            .get_member(drive_index.strict_cast())
            .ok()
            .flatten()
            .or_warn("getting drive data")?;

        match CdDrive::try_from(drive_data) {
            Ok(mut cddrive) => {
                cddrive.add_context(self.cx.clone());
                Some(cddrive)
            }
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
        deviceinfoset,
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
