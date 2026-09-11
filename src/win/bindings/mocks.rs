//! Mock versions of hardware access functions. Allowing for compilation and unit testing
//! on any host

// Prefer re-exported types
use super::{HANDLE, HDEVINFO, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W};

// Only used for function signatures
use super::bindgen::{
    BOOL, CREATEFILE2_EXTENDED_PARAMETERS, GUID, HWND, OVERLAPPED, PCWSTR, PWSTR, SP_DEVINFO_DATA,
};

pub unsafe fn CloseHandle(hobject: HANDLE) -> BOOL {
    todo!("CloseHandle")
}
pub unsafe fn CreateFile2(
    lpfilename: PCWSTR,
    dwdesiredaccess: u32,
    dwsharemode: u32,
    dwcreationdisposition: u32,
    pcreateexparams: *const CREATEFILE2_EXTENDED_PARAMETERS,
) -> HANDLE {
    todo!("CreateFile2")
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
    todo!("DeviceIoControl")
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
