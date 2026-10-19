#![expect(unsafe_code, reason = "bindings to windows API via ffi")]
#![expect(
    clippy::undocumented_unsafe_blocks,
    nonstandard_style,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms,
    reason = "generated bindings to windows API via ffi"
)]
#![expect(
    dead_code,
    reason = "until we split and have an extra use for auto-generated stuff that we don't use"
)]

// TODO: #79 switch from `target_family = "windows"` to simply `cfg(windows)`
#[cfg(all(target_family = "windows", not(any(test, doc))))]
use bindgen::{
    CloseHandle, CreateFile2, DeviceIoControl, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};

#[cfg(any(test, doc, not(target_family = "windows")))]
use mocks::{
    CloseHandle, CreateFile2, DeviceIoControl, SetupDiDestroyDeviceInfoList,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};

use bindgen::{
    BOOL, CDDA, CDROM_READ_TOC_EX, CDROM_TOC, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    DUPLICATE_SAME_ACCESS, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, FALSE,
    FILE_DEVICE_CD_ROM, FILE_SHARE_READ, GENERIC_READ, HANDLE, HDEVINFO, INVALID_HANDLE_VALUE,
    IOCTL_CDROM_RAW_READ, IOCTL_CDROM_READ_TOC_EX, IOCTL_STORAGE_GET_DEVICE_NUMBER, OPEN_EXISTING,
    RAW_READ_INFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    STORAGE_DEVICE_NUMBER, TRACK_DATA,
};

pub(super) use bindgen::{GUID, GUID_DEVINTERFACE_CDROM, PCWSTR};
pub(super) use handles::DeviceDetails;

pub use handles::{DeviceInfoSet, DriveHandle};

#[expect(clippy::allow_attributes, reason = "no guarantees on ordering")]
#[allow(clippy::arbitrary_source_item_ordering, reason = "generated code")]
mod bindgen;

mod cdrom_toc;
mod handles;

#[expect(clippy::allow_attributes, reason = "no guarantees on ordering")]
#[allow(
    clippy::arbitrary_source_item_ordering,
    reason = "consts ordered by value"
)]
mod mocks;
