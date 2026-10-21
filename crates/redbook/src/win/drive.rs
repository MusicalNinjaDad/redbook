//! Handles direct hardware access via Windows APIs

#[cfg(test)]
use std::io;

use tracing_result::Trace;

use crate::{
    Device,
    win::sys::{DeviceInfoSet, DriveHandle},
};
#[cfg(test)]
use crate::{Drive, drive::CdDrive, test_fixtures::albums::TestAlbum};

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

        match DriveHandle::open(&drive_data.path()) {
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

#[cfg(test)]
impl TryFrom<&CdDrive<DriveHandle>> for TestAlbum {
    type Error = io::Error;

    fn try_from(drive: &CdDrive<DriveHandle>) -> Result<Self, Self::Error> {
        let handle = drive.device();
        handle.try_into()
    }
}
