//! Validates that windows ffi bindings do not require updating.
//!
//! Based upon the approach used in [`chrono`](https://github.com/chronotope/chrono/blob/6adaa5240c26fecb7bd9077334a91f8f67f4f3fe/tests/win_bindings.rs)

use std::{fs, path::PathBuf};
use windows_bindgen::Bindgen;

const BINDINGS: [&str; 30] = [
    "CloseHandle",
    "CreateFile2",
    "DeviceIoControl",
    "GetFinalPathNameByHandleW",
    "SetupDiEnumDeviceInterfaces",
    "SetupDiGetClassDevsW",
    "SetupDiGetDeviceInterfaceDetailW",
    "CDDA",
    "CDROM_READ_TOC_EX",
    "CDROM_TOC",
    "GUID_DEVINTERFACE_CDROM",
    "DIGCF_DEVICEINTERFACE",
    "DIGCF_PRESENT",
    "ERROR_INSUFFICIENT_BUFFER",
    "ERROR_NO_MORE_ITEMS",
    "FILE_SHARE_READ",
    "FILE_NAME_NORMALIZED",
    "GENERIC_READ",
    "HANDLE",
    "HDEVINFO",
    "INVALID_HANDLE_VALUE",
    "IOCTL_CDROM_RAW_READ",
    "IOCTL_CDROM_READ_TOC_EX",
    "OPEN_EXISTING",
    "SP_DEVICE_INTERFACE_DATA",
    "SP_DEVICE_INTERFACE_DETAIL_DATA_W",
    "SP_DEVINFO_DATA",
    "RAW_READ_INFO",
    "TRACK_MODE_TYPE",
    "VOLUME_NAME_DOS",
];

#[test]
fn gen_bindings() {
    let src = PathBuf::from("src").join("win").join("bindings.rs");
    let existing = fs::read_to_string(&src).unwrap();

    Bindgen::new()
        .output(&src)
        .filters(BINDINGS)
        .sys()
        .flat()
        .dead_code()
        .write();

    // Check the output is the same as before.
    // Depending on the git configuration the file may have been checked out with `\r\n` newlines or
    // with `\n`. Compare line-by-line to ignore this difference.
    let mut new = fs::read_to_string(&src).unwrap();
    if existing.contains("\r\n") && !new.contains("\r\n") {
        new = new.replace("\n", "\r\n");
    } else if !existing.contains("\r\n") && new.contains("\r\n") {
        new = new.replace("\r\n", "\n");
    }

    similar_asserts::assert_eq!(existing, new);
    if !new.lines().eq(existing.lines()) {
        panic!("generated file `{}` is changed.", src.display());
    }
}
