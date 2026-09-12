//! Validates that windows ffi bindings do not require updating.
//!
//! Based upon the approach used in [`chrono`](https://github.com/chronotope/chrono/blob/6adaa5240c26fecb7bd9077334a91f8f67f4f3fe/tests/win_bindings.rs)

use proc_macro2::TokenStream;
use std::{fs, path::PathBuf};
use syn::{
    Item::{Fn, Macro},
    Signature, parse2,
};
use tempfile::NamedTempFile;
use windows_bindgen::Bindgen;

// Cannot include auto-generated types `GUID`, `PCWSTR`, `TRACKDATA`
// as these are not visible for import.
const BINDINGS: [&str; 29] = [
    "CloseHandle",
    "CreateFile2",
    "DeviceIoControl",
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
#[cfg_attr(miri, ignore)]
fn gen_bindings() {
    let src = PathBuf::from("src")
        .join("win")
        .join("bindings")
        .join("bindgen.rs");
    let existing = fs::read_to_string(&src).unwrap();

    Bindgen::new()
        .output(&src)
        .filters(BINDINGS)
        .sys()
        .flat()
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

#[test]
#[cfg_attr(miri, ignore)]
fn mocks() {
    let tmp_bindings = NamedTempFile::new().unwrap();
    Bindgen::new()
        .output(tmp_bindings.path())
        .filters(BINDINGS)
        .sys()
        .flat()
        .write();
    let bindings_contents = fs::read_to_string(tmp_bindings.path()).unwrap();
    dbg!(&bindings_contents);
    let mocks_src = PathBuf::from("src")
        .join("win")
        .join("bindings")
        .join("mocks.rs");
    let mocks_contents = fs::read_to_string(&mocks_src).unwrap();
    let bindings = syn::parse_file(&bindings_contents).unwrap();
    let mocks = syn::parse_file(&mocks_contents).unwrap();
    let bindings_functions: Vec<_> = bindings
        .items
        .iter()
        .filter_map(|item| {
            if let Macro(mac) = item {
                let defn: TokenStream = mac.mac.tokens.clone().into_iter().skip(2).collect();
                dbg!(&defn);
                let sig: Signature = parse2(defn).unwrap();
                dbg!(&sig);
                Some(sig)
            } else {
                None
            }
        })
        .collect();
    let mocks_functions: Vec<_> = mocks
        .items
        .iter()
        .filter_map(|item| {
            if let Fn(function) = item {
                Some(function)
            } else {
                None
            }
        })
        .collect();
    dbg!(&bindings_functions);
    dbg!(&mocks_functions);
    assert_eq!(bindings_functions.len(), mocks_functions.len())
}
