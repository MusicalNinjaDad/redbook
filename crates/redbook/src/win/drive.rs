//! Handles direct hardware access via Windows APIs

#[cfg(test)]
use std::io;

use crate::{
    Device,
    drive::DriveError,
    win::sys::{DeviceInfoSet, DriveHandle},
};
#[cfg(test)]
use crate::{Drive, drive::CdDrive, test_fixtures::albums::TestAlbum};

impl IntoIterator for DeviceInfoSet {
    type Item = Result<DriveHandle, DriveError>;

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
    type Item = Result<DriveHandle, DriveError>;

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

        let drive_data = match self.infoset.get_member(drive_index) {
            Ok(Some(drive_data)) => drive_data,
            // expected end-of-enumeration condition
            Ok(None) => return None,
            Err(error) => return Some(Err(DriveError::from(error))),
        };

        match DriveHandle::open(&drive_data.path()) {
            Ok(drive) => Some(Ok(drive)),
            Err(error) if error.is_not_ready() => {
                // ERROR_NOT_READY (21) = no disc in drive: expected, skip to the next device
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
