//! Handles direct hardware access via Windows APIs

#[cfg(test)]
use std::io;

use crate::{
    Device,
    drive::error::DeviceError,
    win::sys::{DeviceInfoSet, DriveHandle},
};
#[cfg(test)]
use crate::{Drive, drive::CdDrive, test_fixtures::albums::TestAlbum};

impl IntoIterator for DeviceInfoSet {
    type Item = Result<DriveHandle, DeviceError>;

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
    /// Items are `Result`s so that enumeration & drive-construction failures are propagated
    /// rather than silently ending iteration (issue #50); `None` is reserved for the expected
    /// end-of-enumeration condition (`ERROR_NO_MORE_ITEMS`).
    type Item = Result<DriveHandle, DeviceError>;

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

        let drive_data = match self.infoset.get_member(drive_index).transpose()? {
            Ok(drive_data) => drive_data,
            Err(error) => return Some(Err(error)),
        };

        match DriveHandle::open(&drive_data.path()) {
            Ok(drive) => Some(Ok(drive)),
            Err(DeviceError::NotReady(_)) => {
                // no disc in drive: skip to the next device
                drop(debug);
                self.next()
            }
            Err(error) => Some(Err(error)),
        }
    }
}

#[cfg(test)]
impl TryFrom<&CdDrive<DriveHandle>> for TestAlbum {
    type Error = io::Error;

    fn try_from(drive: &CdDrive<DriveHandle>) -> Result<Self, Self::Error> {
        let handle = drive.device();
        handle.try_into()
    }
}
