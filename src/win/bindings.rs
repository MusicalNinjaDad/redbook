windows_link::link!("kernel32.dll" "system" fn CloseHandle(hobject : HANDLE) -> BOOL);
windows_link::link!("kernel32.dll" "system" fn CreateFile2(lpfilename : PCWSTR, dwdesiredaccess : u32, dwsharemode : u32, dwcreationdisposition : u32, pcreateexparams : *const CREATEFILE2_EXTENDED_PARAMETERS) -> HANDLE);
windows_link::link!("kernel32.dll" "system" fn DeviceIoControl(hdevice : HANDLE, dwiocontrolcode : u32, lpinbuffer : *const core::ffi::c_void, ninbuffersize : u32, lpoutbuffer : *mut core::ffi::c_void, noutbuffersize : u32, lpbytesreturned : *mut u32, lpoverlapped : *mut OVERLAPPED) -> BOOL);
windows_link::link!("kernel32.dll" "system" fn GetFinalPathNameByHandleW(hfile : HANDLE, lpszfilepath : PWSTR, cchfilepath : u32, dwflags : u32) -> u32);
windows_link::link!("setupapi.dll" "system" fn SetupDiEnumDeviceInterfaces(deviceinfoset : HDEVINFO, deviceinfodata : *const SP_DEVINFO_DATA, interfaceclassguid : *const GUID, memberindex : u32, deviceinterfacedata : *mut SP_DEVICE_INTERFACE_DATA) -> BOOL);
windows_link::link!("setupapi.dll" "system" fn SetupDiGetClassDevsW(classguid : *const GUID, enumerator : PCWSTR, hwndparent : HWND, flags : u32) -> HDEVINFO);
windows_link::link!("setupapi.dll" "system" fn SetupDiGetDeviceInterfaceDetailW(deviceinfoset : HDEVINFO, deviceinterfacedata : *const SP_DEVICE_INTERFACE_DATA, deviceinterfacedetaildata : *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W, deviceinterfacedetaildatasize : u32, requiredsize : *mut u32, deviceinfodata : *mut SP_DEVINFO_DATA) -> BOOL);
pub type BOOL = i32;
pub const CDDA: TRACK_MODE_TYPE = 2;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CDROM_READ_TOC_EX {
    pub _bitfield: u8,
    pub SessionTrack: u8,
    pub Reserved2: u8,
    pub Reserved3: u8,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDROM_TOC {
    pub Length: [u8; 2],
    pub FirstTrack: u8,
    pub LastTrack: u8,
    pub TrackData: [TRACK_DATA; 100],
}
impl Default for CDROM_TOC {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct CREATEFILE2_EXTENDED_PARAMETERS {
    pub dwSize: u32,
    pub dwFileAttributes: u32,
    pub dwFileFlags: u32,
    pub dwSecurityQosFlags: u32,
    pub lpSecurityAttributes: LPSECURITY_ATTRIBUTES,
    pub hTemplateFile: HANDLE,
}
pub const DIGCF_DEVICEINTERFACE: i32 = 16;
pub const DIGCF_PRESENT: i32 = 2;
pub const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
pub const ERROR_NO_MORE_ITEMS: i32 = 259;
pub const FILE_NAME_NORMALIZED: i32 = 0;
pub const FILE_SHARE_READ: i32 = 1;
pub const GENERIC_READ: u32 = 2147483648;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}
pub const GUID_DEVINTERFACE_CDROM: GUID = GUID {
    data1: 0x53f56308,
    data2: 0xb6bf,
    data3: 0x11d0,
    data4: [148, 242, 0, 160, 201, 30, 251, 139],
};
pub type HANDLE = *mut core::ffi::c_void;
pub type HDEVINFO = *mut core::ffi::c_void;
pub type HWND = *mut core::ffi::c_void;
pub const INVALID_HANDLE_VALUE: HANDLE = -1 as _;
pub const IOCTL_CDROM_RAW_READ: i32 = 147518;
pub const IOCTL_CDROM_READ_TOC_EX: i32 = 147540;
pub type LPSECURITY_ATTRIBUTES = *mut SECURITY_ATTRIBUTES;
pub const OPEN_EXISTING: i32 = 3;
#[repr(C)]
#[derive(Clone, Copy)]
pub struct OVERLAPPED {
    pub Internal: usize,
    pub InternalHigh: usize,
    pub Anonymous: OVERLAPPED_0,
    pub hEvent: HANDLE,
}
impl Default for OVERLAPPED {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy)]
pub union OVERLAPPED_0 {
    pub Anonymous: OVERLAPPED_0_0,
    pub Pointer: *mut core::ffi::c_void,
}
impl Default for OVERLAPPED_0 {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct OVERLAPPED_0_0 {
    pub Offset: u32,
    pub OffsetHigh: u32,
}
pub type PCWSTR = *const u16;
pub type PWSTR = *mut u16;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct RAW_READ_INFO {
    pub DiskOffset: i64,
    pub SectorCount: u32,
    pub TrackMode: TRACK_MODE_TYPE,
}
pub const RawWithC2: TRACK_MODE_TYPE = 4;
pub const RawWithC2AndSubCode: TRACK_MODE_TYPE = 3;
pub const RawWithSubCode: TRACK_MODE_TYPE = 5;
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: u32,
    pub lpSecurityDescriptor: *mut core::ffi::c_void,
    pub bInheritHandle: BOOL,
}
#[repr(C, packed(1))]
#[cfg(target_arch = "x86")]
#[derive(Clone, Copy, Default)]
pub struct SP_DEVICE_INTERFACE_DATA {
    pub cbSize: u32,
    pub InterfaceClassGuid: GUID,
    pub Flags: u32,
    pub Reserved: usize,
}
#[repr(C)]
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
#[derive(Clone, Copy, Default)]
pub struct SP_DEVICE_INTERFACE_DATA {
    pub cbSize: u32,
    pub InterfaceClassGuid: GUID,
    pub Flags: u32,
    pub Reserved: usize,
}
#[repr(C, packed(1))]
#[cfg(target_arch = "x86")]
#[derive(Clone, Copy)]
pub struct SP_DEVICE_INTERFACE_DETAIL_DATA_W {
    pub cbSize: u32,
    pub DevicePath: [u16; 1],
}
#[cfg(target_arch = "x86")]
impl Default for SP_DEVICE_INTERFACE_DETAIL_DATA_W {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C)]
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
#[derive(Clone, Copy)]
pub struct SP_DEVICE_INTERFACE_DETAIL_DATA_W {
    pub cbSize: u32,
    pub DevicePath: [u16; 1],
}
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
impl Default for SP_DEVICE_INTERFACE_DETAIL_DATA_W {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
#[repr(C, packed(1))]
#[cfg(target_arch = "x86")]
#[derive(Clone, Copy, Default)]
pub struct SP_DEVINFO_DATA {
    pub cbSize: u32,
    pub ClassGuid: GUID,
    pub DevInst: u32,
    pub Reserved: usize,
}
#[repr(C)]
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
#[derive(Clone, Copy, Default)]
pub struct SP_DEVINFO_DATA {
    pub cbSize: u32,
    pub ClassGuid: GUID,
    pub DevInst: u32,
    pub Reserved: usize,
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct TRACK_DATA {
    pub Reserved: u8,
    pub _bitfield: u8,
    pub TrackNumber: u8,
    pub Reserved1: u8,
    pub Address: [u8; 4],
}
impl Default for TRACK_DATA {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub type TRACK_MODE_TYPE = i32;
pub const VOLUME_NAME_DOS: i32 = 0;
pub const XAForm2: TRACK_MODE_TYPE = 1;
pub const YellowMode2: TRACK_MODE_TYPE = 0;
