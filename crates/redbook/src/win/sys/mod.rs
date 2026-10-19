// TODO: #79 switch from `target_family = "windows"` to simply `cfg(windows)`
#[cfg(all(target_family = "windows", not(any(test, doc))))]
use bindgen::{
    CloseHandle, CreateFile2, DeviceIoControl, DuplicateHandle, GetCurrentProcess,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW,
};

#[cfg(any(test, doc, not(target_family = "windows")))]
use mocks::{
    CloseHandle, CreateFile2, DeviceIoControl, DuplicateHandle, GetCurrentProcess,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW,
};

#[cfg(any(test, doc, not(target_family = "windows")))]
use bindgen::{
    CREATEFILE2_EXTENDED_PARAMETERS, FILE_FLAG_OVERLAPPED, HWND, OVERLAPPED, SP_DEVINFO_DATA,
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
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks,
    nonstandard_style,
    clippy::upper_case_acronyms,
    reason = "generated bindings to windows API via ffi"
)]
#[expect(
    dead_code,
    reason = "not win: functions mocked, win: some consts only needed for mocks & tests"
)]
mod bindgen;

/// Additional functions on CDROM_TOC & TRACK_DATA to allow for interconversion without leaking
/// windows-specific structs into the main API.
mod cdrom_toc;

#[expect(unsafe_code, reason = "type safe wrappers around HANDLE & HDEVINFO")]
mod handles;

#[expect(clippy::allow_attributes, reason = "no guarantees on ordering")]
#[allow(
    clippy::arbitrary_source_item_ordering,
    reason = "consts ordered by value"
)]
#[expect(
    unsafe_code,
    clippy::undocumented_unsafe_blocks, // TODO: remove this expectation
    nonstandard_style,
    clippy::too_many_arguments,
    reason = "mocked versions of generated bindings to windows API via ffi"
)]
#[cfg(any(test, doc, not(target_family = "windows")))]
mod mocks;
