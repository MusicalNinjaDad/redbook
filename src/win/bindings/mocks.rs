//! Mock versions of hardware access functions. Allowing for compilation and unit testing
//! on any host

use std::path::PathBuf;
use std::{io, slice};

use crate::test_fixtures::albums::TestAlbum::{self, *};
use crate::win::bindings::CDROM_READ_TOC_EX;

use super::super::{MAX_PATH_CHARS, convert::WinString};

// Prefer re-exported types
use super::{HANDLE, HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W};

// Only used for function signatures
use super::bindgen::{
    BOOL, CREATEFILE2_EXTENDED_PARAMETERS, GUID, HWND, OVERLAPPED, PCWSTR, PWSTR, SP_DEVINFO_DATA,
};

const DEFINITELY_MAYBE: HANDLE = 1 as _;

impl TryFrom<HANDLE> for TestAlbum {
    type Error = io::Error;

    fn try_from(handle: HANDLE) -> Result<Self, Self::Error> {
        match handle {
            DEFINITELY_MAYBE => Ok(DefinitelyMaybe),
            _ => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "invalid album handle",
            )),
        }
    }
}

pub unsafe fn CloseHandle(hobject: HANDLE) -> BOOL {
    0
}
/// # SAFETY:
/// - `lpfilename` must be a valid pointer to a `&[16]` which can be interpreted as
///   a null-terminated, utf-16 encoded String, of at most [MAX_PATH_CHARS].
///   The best way to achieve this is by passing the result of [`WinString::as_pcwstr()`]
pub unsafe fn CreateFile2(
    lpfilename: PCWSTR,
    dwdesiredaccess: u32,
    dwsharemode: u32,
    dwcreationdisposition: u32,
    pcreateexparams: *const CREATEFILE2_EXTENDED_PARAMETERS,
) -> HANDLE {
    let pcwstr = unsafe { slice::from_raw_parts(lpfilename, MAX_PATH_CHARS) };
    let win_path = WinString::from(pcwstr).to_string();
    let path = PathBuf::from(win_path.strip_prefix(r"\\.\").unwrap());

    match TestAlbum::try_from(&path).unwrap() {
        DefinitelyMaybe => DEFINITELY_MAYBE,
        TheWallDisc1 => todo!("w1"),
        TheWallDisc2 => todo!("w2"),
    }
}
pub unsafe fn DeviceIoControl(
    hdevice: HANDLE,
    dwiocontrolcode: u32,
    lpinbuffer: *const core::ffi::c_void,
    ninbuffersize: u32,
    lpoutbuffer: *mut core::ffi::c_void,
    noutbuffersize: u32,
    lpbytesreturned: *mut u32,
    lpoverlapped: *mut OVERLAPPED,
) -> BOOL {
    match (hdevice, dwiocontrolcode) {
        (DEFINITELY_MAYBE, CDROM_READ_TOC_EX) => {
            let toc = DefinitelyMaybe.load_cdrom_toc();
            todo!("dm")
        }
        _ => todo!("mock DeviceIoControl")
    }
}
pub unsafe fn GetFinalPathNameByHandleW(
    hfile: HANDLE,
    lpszfilepath: PWSTR,
    cchfilepath: u32,
    dwflags: u32,
) -> u32 {
    todo!("create mock")
}
pub unsafe fn SetupDiEnumDeviceInterfaces(
    deviceinfoset: HDEVINFO,
    deviceinfodata: *const SP_DEVINFO_DATA,
    interfaceclassguid: *const GUID,
    memberindex: u32,
    deviceinterfacedata: *mut SP_DEVICE_INTERFACE_DATA,
) -> BOOL {
    todo!("create mock")
}
pub unsafe fn SetupDiGetClassDevsW(
    classguid: *const GUID,
    enumerator: PCWSTR,
    hwndparent: HWND,
    flags: u32,
) -> HDEVINFO {
    todo!("create mock")
}
pub unsafe fn SetupDiGetDeviceInterfaceDetailW(
    deviceinfoset: HDEVINFO,
    deviceinterfacedata: *const SP_DEVICE_INTERFACE_DATA,
    deviceinterfacedetaildata: *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    deviceinterfacedetaildatasize: u32,
    requiredsize: *mut u32,
    deviceinfodata: *mut SP_DEVINFO_DATA,
) -> BOOL {
    todo!("create mock")
}
