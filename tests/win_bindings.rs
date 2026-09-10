//! Validates that windows ffi bindings do not require updating.
//!
//! Based upon the approach used in [`chrono`](https://github.com/chronotope/chrono/blob/6adaa5240c26fecb7bd9077334a91f8f67f4f3fe/tests/win_bindings.rs)

use std::fs;
use windows_bindgen::bindgen;

const BINDINGS: &str = "src/win/bindings.rs";

#[test]
fn gen_bindings() {
    let existing = fs::read_to_string(BINDINGS).unwrap();

    bindgen([
        "--out",
        BINDINGS,
        "--flat",
        "--sys",
        "--filter",
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
    ]);

    // Check the output is the same as before.
    // Depending on the git configuration the file may have been checked out with `\r\n` newlines or
    // with `\n`. Compare line-by-line to ignore this difference.
    let mut new = fs::read_to_string(BINDINGS).unwrap();
    if existing.contains("\r\n") && !new.contains("\r\n") {
        new = new.replace("\n", "\r\n");
    } else if !existing.contains("\r\n") && new.contains("\r\n") {
        new = new.replace("\r\n", "\n");
    }

    similar_asserts::assert_eq!(existing, new);
    if !new.lines().eq(existing.lines()) {
        panic!("generated file `{BINDINGS}` is changed.");
    }
}
