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
#![cfg_attr(
    any(test, doc, not(target_family = "windows")),
    expect(unused_imports, reason = "need to tidy up cfg windows gates")
)]

mod bindgen;
#[expect(unused_variables)]
mod mocks;

#[cfg(all(target_family = "windows", not(any(test, doc))))]
pub(crate) use bindgen::*;

#[cfg(any(test, doc, not(target_family = "windows")))]
pub(crate) use mocks::{
    CloseHandle, CreateFile2, DeviceIoControl, GetFinalPathNameByHandleW,
    SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
};

#[cfg(any(test, doc, not(target_family = "windows")))]
pub use bindgen::{
    CDDA, CDROM_READ_TOC_EX, CDROM_TOC, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
    ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, FILE_NAME_NORMALIZED, FILE_SHARE_READ,
    GENERIC_READ, GUID, GUID_DEVINTERFACE_CDROM, HANDLE, HDEVINFO, INVALID_HANDLE_VALUE,
    IOCTL_CDROM_RAW_READ, IOCTL_CDROM_READ_TOC_EX, OPEN_EXISTING, PCWSTR, RAW_READ_INFO,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA, TRACK_DATA,
    TRACK_MODE_TYPE, VOLUME_NAME_DOS,
};
