//! Mock versions of hardware access functions. Allowing for compilation and unit testing
//! on any host

use std::path::PathBuf;
use std::{io, slice};

use crate::test_fixtures::albums::TestAlbum::{self, *};
use crate::win::bindings::{
    ERROR_INSUFFICIENT_BUFFER, GUID_DEVINTERFACE_CDROM, IOCTL_CDROM_READ_TOC_EX,
};
use crate::win::convert::Guid;
use crate::win::drive::DeviceDetails;
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

/// (Optionally) set `last_os_err` to `errno` then return 0 to signify failure
macro_rules! failure {
    () => {
        return 0
    };
    ($errno:expr) => {{
        errno::set_errno(errno::Errno($errno));
        return 0;
    }};
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
    let album = match (deviceinfoset, memberindex) {
        (ALL_ALBUMS, 0) => DefinitelyMaybe as u32,
        _ => todo!("mock SetupDiEnumDeviceInterfaces"),
    };
    let data = SP_DEVICE_INTERFACE_DATA {
        cbSize: const { size_of::<SP_DEVICE_INTERFACE_DATA>() as u32 },
        InterfaceClassGuid: GUID {
            data1: album,
            data2: 0,
            data3: 0,
            data4: [0; _],
        },
        Flags: 0,
        Reserved: 0,
    };
    unsafe { *deviceinterfacedata = data };
    1
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
/// This function can be used in one of two ways. Usually in sequence:
///
/// 1. Get the required buffer size. Call SetupDiGetDeviceInterfaceDetail with a
///    NULLDeviceInterfaceDetailData pointer, a DeviceInterfaceDetailDataSize of zero,
///    and a valid RequiredSize variable. In response to such a call, this function returns
///    the required buffer size at RequiredSize and fails with GetLastError
///    returning ERROR_INSUFFICIENT_BUFFER.
/// 2. Allocate an appropriately sized buffer and call the function again to get the
///    interface details.
///
/// # SAFETY:
/// 1.  If getting the required buffer size:
///     - `deviceinterfacedetaildata` must be NULL
///     - `deviceinterfacedetailsize` must be 0
///     - `requiredsize` must be a valid pointer
/// 2.  If getting the device interface data
///     - `deviceinterfacedetaildata` must be a valid pointer
///     - `deviceinterfacedetaildata.cbSize` must be set to `size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>()`
///       before calling this function. The cbSize member always contains the size of the fixed part
///       of the data structure, not a size reflecting the variable-length string at the end.
///     - `deviceinterfacedetaildata.DevicePath` must be sufficiently sized, for the returned
///       path INCLUDING a terminating NULL character.
///     - `deviceinterfacedetailsize` must be size_of_val(deviceinterfacedetaildata)
///     - `requiredsize` must be NULL
///
/// # Arguments
/// - `[in] deviceinfoset`: A pointer to the device information set that contains the interface
///   for which to retrieve details. This handle is typically returned by [SetupDiGetClassDevsW].
/// - `[in] deviceinterfacedata`: A pointer to an [SP_DEVICE_INTERFACE_DATA] structure that specifies
///   the interface in DeviceInfoSet for which to retrieve details. A pointer of this type is
///   typically returned by [SetupDiEnumDeviceInterfaces].
/// - `[out, optional] deviceinterfacedetaildata`: A pointer to an [SP_DEVICE_INTERFACE_DETAIL_DATA_W]
///   structure to receive information about the specified interface. This parameter is optional
///   and can be NULL.
/// - `[in] deviceinterfacedetaildatasize`: The size of the DeviceInterfaceDetailData buffer.
/// - `[out, optional] requiredsize`: receives the required size of the DeviceInterfaceDetailData
///   buffer. This size includes the size of the fixed part of the structure plus the number of
///   bytes required for the variable-length device path string. This parameter is optional and
///   can be NULL.
pub unsafe fn SetupDiGetDeviceInterfaceDetailW(
    deviceinfoset: HDEVINFO,
    deviceinterfacedata: *const SP_DEVICE_INTERFACE_DATA,
    deviceinterfacedetaildata: *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    deviceinterfacedetaildatasize: u32,
    requiredsize: *mut u32,
    deviceinfodata: *mut SP_DEVINFO_DATA,
) -> BOOL {
    let album = match (
        deviceinfoset,
        unsafe { *deviceinterfacedata }.InterfaceClassGuid.data1,
    ) {
        (ALL_ALBUMS, id) if id == DefinitelyMaybe as u32 => DefinitelyMaybe,
        _ => todo!("mock other albums"),
    };
    let album_path = format!(r"\\.\{}", album.assets_path().display());
    let path_len = album_path.len();
    match (
        deviceinterfacedetaildata.is_null(),
        deviceinterfacedetaildatasize,
        requiredsize.is_null(),
    ) {
        (true, 0, false) => {
            dbg!("get required size");
            // This potentially slightly oversizes the requirement. This is acceptable for
            // testing purposes to avoid the complexity of calculating the size required when
            // *replacing* the default buffer of [u16; 1] with a sufficiently large buffer.
            let size = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() + path_len;
            // SAFETY: Have validated pointer is not NULL, mock runs on debug build so as u32
            // will panic on overflow.
            unsafe { *requiredsize = size as u32 };
            failure!(ERROR_INSUFFICIENT_BUFFER)
        }
        (false, s, true) => {
            let mut data = DeviceDetails::default();
            match data.set_path(&WinString::from(album_path)) {
                Ok(words) if words == path_len + 1 => {
                    // SAFETY:
                    // - pointer is not null (match arm)
                    // - DeviceDetails is directly compatible with SP_DEVICE_INTERFACE_DETAIL_DATA_W
                    unsafe { *(deviceinterfacedetaildata as *mut DeviceDetails) = data };
                    todo!("return data")
                }
                _ => todo!("setting path failed"),
            };
        }
        _ => panic!("invalid call to SetupDiGetDeviceInterfaceDetailW"),
    }
}
