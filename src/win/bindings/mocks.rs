//! Mock versions of hardware access functions. Allowing for compilation and unit testing
//! on any host

use std::path::PathBuf;
use std::{io, slice};

use crate::test_fixtures::albums::TestAlbum::{self, *};
use crate::win::bindings::{GUID_DEVINTERFACE_CDROM, IOCTL_CDROM_READ_TOC_EX};
use crate::win::convert::Guid;
use crate::win::toc::CDROM_TOC;

use super::super::{MAX_PATH_CHARS, convert::WinString};

// Prefer re-exported types
use super::{HANDLE, HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W};

// Only used for function signatures
use super::bindgen::{
    BOOL, CREATEFILE2_EXTENDED_PARAMETERS, GUID, HWND, OVERLAPPED, PCWSTR, SP_DEVINFO_DATA,
};

const DEFINITELY_MAYBE: HANDLE = 1 as _;
const THE_WALL_1: HANDLE = 2 as _;
const THE_WALL_2: HANDLE = 3 as _;
const ALL_ALBUMS: HDEVINFO = 4 as _;

impl TryFrom<HANDLE> for TestAlbum {
    type Error = io::Error;

    fn try_from(handle: HANDLE) -> Result<Self, Self::Error> {
        match handle {
            DEFINITELY_MAYBE => Ok(DefinitelyMaybe),
            THE_WALL_1 => Ok(TheWallDisc1),
            THE_WALL_2 => Ok(TheWallDisc2),
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
    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "deference pointer arithmetic"
    )]
    let path_len = (0..MAX_PATH_CHARS)
        .find(|&i| unsafe { *lpfilename.add(i) == 0 })
        .expect("null termination before MAX_PATH_CHARS");
    let pcwstr = unsafe { slice::from_raw_parts(lpfilename, path_len) };
    let win_path = WinString::from(pcwstr).to_string();
    let path = PathBuf::from(win_path.strip_prefix(r"\\.\").unwrap());

    match TestAlbum::try_from(&path).unwrap() {
        DefinitelyMaybe => DEFINITELY_MAYBE,
        TheWallDisc1 => THE_WALL_1,
        TheWallDisc2 => THE_WALL_2,
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
    let toc = match hdevice {
        DEFINITELY_MAYBE if dwiocontrolcode == IOCTL_CDROM_READ_TOC_EX as u32 => {
            DefinitelyMaybe.load_cdrom_toc()
        }
        THE_WALL_1 if dwiocontrolcode == IOCTL_CDROM_READ_TOC_EX as u32 => {
            TheWallDisc1.load_cdrom_toc()
        }
        THE_WALL_2 if dwiocontrolcode == IOCTL_CDROM_READ_TOC_EX as u32 => {
            TheWallDisc2.load_cdrom_toc()
        }
        _ => todo!("mock DeviceIoControl"),
    };
    assert_eq!(noutbuffersize as usize, size_of_val(&toc));
    unsafe { *(lpoutbuffer as *mut CDROM_TOC) = toc };
    1
}
pub unsafe fn SetupDiEnumDeviceInterfaces(
    deviceinfoset: HDEVINFO,
    deviceinfodata: *const SP_DEVINFO_DATA,
    interfaceclassguid: *const GUID,
    memberindex: u32,
    deviceinterfacedata: *mut SP_DEVICE_INTERFACE_DATA,
) -> BOOL {
    match (deviceinfoset, memberindex) {
        (ALL_ALBUMS, 0) => todo!("dm"),
        _ => todo!("mock SetupDiEnumDeviceInterfaces"),
    }
}
pub unsafe fn SetupDiGetClassDevsW(
    classguid: *const GUID,
    enumerator: PCWSTR,
    hwndparent: HWND,
    flags: u32,
) -> HDEVINFO {
    let classguid = Guid(unsafe { *classguid });
    match classguid {
        guid if guid == Guid(GUID_DEVINTERFACE_CDROM) => ALL_ALBUMS,
        _ => todo!("unknown guid"),
    }
}
pub unsafe fn SetupDiGetDeviceInterfaceDetailW(
    deviceinfoset: HDEVINFO,
    deviceinterfacedata: *const SP_DEVICE_INTERFACE_DATA,
    deviceinterfacedetaildata: *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    deviceinterfacedetaildatasize: u32,
    requiredsize: *mut u32,
    deviceinfodata: *mut SP_DEVINFO_DATA,
) -> BOOL {
    todo!("mock SetupDiGetDeviceInterfaceDetailW")
}
