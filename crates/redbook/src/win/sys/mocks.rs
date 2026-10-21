//! Mock versions of hardware access functions. Allowing for compilation and unit testing
//! on any host

use std::path::PathBuf;
use std::{io, slice};

use crate::{
    test_fixtures::albums::TestAlbum::{self, *},
    win::{
        MAX_PATH_CHARS,
        convert::{Guid, WinString},
    },
};

use super::{
    BOOL, CDROM_READ_TOC_EX, CDROM_TOC, CREATEFILE2_EXTENDED_PARAMETERS, DUPLICATE_SAME_ACCESS,
    ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, FALSE, FILE_DEVICE_CD_ROM,
    FILE_FLAG_OVERLAPPED, GUID, GUID_DEVINTERFACE_CDROM, HANDLE, HDEVINFO, HWND,
    INVALID_HANDLE_VALUE, IOCTL_CDROM_READ_TOC_EX, IOCTL_STORAGE_GET_DEVICE_NUMBER, OVERLAPPED,
    PCWSTR, SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SP_DEVINFO_DATA,
    STORAGE_DEVICE_NUMBER, handles::DeviceDetails,
};

const DEFINITELY_MAYBE: HANDLE = 1 as _;
const THE_WALL_1: HANDLE = 2 as _;
const THE_WALL_2: HANDLE = 3 as _;
const ALL_ALBUMS: HDEVINFO = 4 as _;
const BAT_OUT_OF_HELL_REVAMPED: HANDLE = 5 as _;
const THE_BOY_WHO_KNEW_TOO_MUCH: HANDLE = 6 as _;

impl TryFrom<HANDLE> for TestAlbum {
    type Error = io::Error;

    fn try_from(handle: HANDLE) -> Result<Self, Self::Error> {
        match handle {
            DEFINITELY_MAYBE => Ok(DefinitelyMaybe),
            THE_WALL_1 => Ok(TheWallDisc1),
            THE_WALL_2 => Ok(TheWallDisc2),
            BAT_OUT_OF_HELL_REVAMPED => Ok(BatOutOfHellRevamped),
            THE_BOY_WHO_KNEW_TOO_MUCH => Ok(TheBoyWhoKnewTooMuch),
            _ => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "invalid album handle",
            )),
        }
    }
}

impl From<TestAlbum> for HANDLE {
    fn from(album: TestAlbum) -> Self {
        match album {
            DefinitelyMaybe => DEFINITELY_MAYBE,
            TheWallDisc1 => THE_WALL_1,
            TheWallDisc2 => THE_WALL_2,
            BatOutOfHellRevamped => BAT_OUT_OF_HELL_REVAMPED,
            TheBoyWhoKnewTooMuch => THE_BOY_WHO_KNEW_TOO_MUCH,
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

/// Set `last_os_err` to `0` then return a success value, by default: `1`
macro_rules! success {
    () => {{
        errno::set_errno(errno::Errno(0));
        return 1;
    }};
    ($success_code:expr) => {{
        errno::set_errno(errno::Errno(0));
        return $success_code;
    }};
}

pub unsafe fn CloseHandle(
    #[expect(unused_variables, reason = "always succeeds")] hobject: HANDLE,
) -> BOOL {
    // TODO: Maybe mock this with some thread-local RefCell HashSet or similar to
    // allow for tests which validate closure in error cases & ensure no double closures.
    success!()
}

pub unsafe fn SetupDiDestroyDeviceInfoList(
    #[expect(unused_variables, reason = "always succeeds")] deviceinfoset: HDEVINFO,
) -> BOOL {
    // TODO: Maybe mock this with some thread-local RefCell HashSet or similar to
    // allow for tests which validate closure in error cases & ensure no double closures.
    success!()
}

/// Creates or opens a file or I/O device. (See also [Ms Learn][docs_CreateFile2])
///
/// # SAFETY:
/// - `lpfilename` must be a valid pointer to a `&[16]` which can be interpreted as
///   a null-terminated, utf-16 encoded String, of at most [MAX_PATH_CHARS].
///   The best way to achieve this is by passing the result of [`WinString::as_pcwstr()`]
/// - The returned handle must be closed via [CloseHandle] when no longer needed. It is recommended
///   to store it in a custom struct and implement [`Drop`] to close the handle. Particular care
///   should be taken if errors occur between opening the handle and creating the wrapping struct.
///
/// # Notes
/// - Overlapped access is not supported. `pcreateexparams.dwFileFlags` must not include [FILE_FLAG_OVERLAPPED]
///
/// # Returns / `last_os_error`
/// - on success: A handle that can be passed to [DeviceIoControl]
/// - on failure: [INVALID_HANDLE_VALUE][super::INVALID_HANDLE_VALUE].
///   To get extended error information, call [`last_os_error`][io::Error::last_os_error].
///
/// # Arguments
/// - `[in] lpfilename`: The name of the file or device to be created or opened.
/// - `[in] dwdesiredaccess`: The requested access to the file or device. For opening a CD drive
///   use [GENERIC_READ][super::GENERIC_READ]
/// - `[in] dwShareMode`: The requested sharing mode of the file or device. For opening a CD drive
///   use [`FILE_SHARE_READ`][super::FILE_SHARE_READ]
/// - `[in] dwcreationdisposition`: An action to take on a file or device that exists or does not
///   exist. For opening a CD drive use [OPEN_EXISTING][super::OPEN_EXISTING]
/// - `[in, optional] pcreateexparams`: Pointer to an optional [CREATEFILE2_EXTENDED_PARAMETERS]
///   structure. Not supported in mock. See Note.
///
/// [docs_CreateFile2]: https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2
pub unsafe fn CreateFile2(
    lpfilename: PCWSTR,
    #[expect(unused_variables, reason = "irrelevant for mock")] dwdesiredaccess: u32,
    #[expect(unused_variables, reason = "irrelevant for mock")] dwsharemode: u32,
    #[expect(unused_variables, reason = "irrelevant for mock")] dwcreationdisposition: u32,
    pcreateexparams: *const CREATEFILE2_EXTENDED_PARAMETERS,
) -> HANDLE {
    // Overlapped IO is unsupported
    if !pcreateexparams.is_null() {
        assert_eq!(
            unsafe { *pcreateexparams }.dwFileFlags & FILE_FLAG_OVERLAPPED.strict_cast::<u32>(),
            0
        )
    };

    #[expect(
        clippy::multiple_unsafe_ops_per_block,
        reason = "deference pointer arithmetic"
    )]
    let path_len = (0..MAX_PATH_CHARS)
        .find(|&i| unsafe { *lpfilename.add(i) == 0 })
        .expect("null termination before MAX_PATH_CHARS");
    let pcwstr = unsafe { slice::from_raw_parts(lpfilename, path_len) };

    let win_path = WinString::from(pcwstr).to_string();
    dbg!(&win_path);
    let path = PathBuf::from(win_path.strip_prefix(r"\\.\").unwrap());
    dbg!(&path);

    match TestAlbum::try_from(&path).unwrap() {
        DefinitelyMaybe => DEFINITELY_MAYBE,
        TheWallDisc1 => THE_WALL_1,
        TheWallDisc2 => THE_WALL_2,
        BatOutOfHellRevamped => BAT_OUT_OF_HELL_REVAMPED,
        TheBoyWhoKnewTooMuch => THE_BOY_WHO_KNEW_TOO_MUCH,
    }
}

/// Sends a control code directly to a specified device driver, causing the corresponding device
/// to perform the corresponding operation. (See also [MS learn][docs_DeviceIoControl])
///
/// # SAFETY
/// - `hdevice` must be a valid handle to an open resource of the correct type and with the correct
///   access flags for `dwiocontrolcode`.
/// - `hdevice` must NOT have been opened with `FILE_FLAG_OVERLAPPED` (currently unsupported).
/// - `ninbuffersize` must be `size_of::<LPINBUFFERTYPE>()`
/// - `noutbuffersize` must be `size_of::<LPOUTBUFFERTYPE>()`
/// - `lpinbuffer` & `lpoutbuffer` must be correct for the requested `dwiocontrolcode`.
/// - For `dwiocontrolcode` [`IOCTL_CDROM_READ_TOC_EX`],
///   (see also [MS learn][docs_IOCTL_CDROM_READ_TOC_EX]):
///     - `lpinbuffer`: points to a buffer of type [`CDROM_READ_TOC_EX`][super::CDROM_READ_TOC_EX]
///       whose contents indicate what information should be retrieved from the target device
///     - `lpoutbuffer`: usually points to a [`CDROM_TOC`] **see Notes** (see also [MS learn][docs_CDROM_TOC]).
/// - For `dwiocontrolcode` [`IOCTL_STORAGE_GET_DEVICE_NUMBER`],
///   (see also [MS Learn][docs_IOCTL_STORAGE_GET_DEVICE_NUMBER]):
///     - `lpinbuffer` is NULL
///     - `ninbuffersize` is 0
///     - `lpoutbuffer`: points to a [`STORAGE_DEVICE_NUMBER`]
/// - `lpbytesreturned` cannot be NULL. See Notes for reason.
/// - `lpoverlapped` MUST be NULL. See Notes for reason.
///
/// # Notes
/// - The mock version supports the following control codes:
///     - [`IOCTL_CDROM_READ_TOC_EX`]
///     - [`IOCTL_STORAGE_GET_DEVICE_NUMBER`]
/// - The current win_bindgen generated [`CDROM_READ_TOC_EX`][super::CDROM_READ_TOC_EX] does not
///   expose the `format` field. Instead providing `_bitfeld: u8` with the first 4 bits representing
///   `format`. Adjusting these will affect the requirements placed on `lpoutbuffer` & `noutbuffersize`
/// - We currently do not support overlapped operations and:
///     - If `lpoverlapped` is NULL, `lpbytesreturned` cannot be NULL.
///       Even when an operation returns no output data and `lpoutbuffer` is NULL,
///       `DeviceIoControl` makes use of `lpbytesreturned`.
///       After such an operation, the value of `lpbytesreturned` is meaningless.
///     - If `lpoverlapped` is not NULL, `lpbytesreturned` is not NULL and the operation returns data,
///       `lpbytesreturned` is meaningless until the overlapped operation has completed.
///       To retrieve the number of bytes returned, call `GetOverlappedResult`.
///       If `hdevice` is associated with an I/O completion port, you can retrieve the number of bytes
///       returned by calling `GetQueuedCompletionStatus`.
///
/// # Returns / `last_os_error`
/// - If the output buffer is too small to receive any data, the call fails, sets
///   `ERROR_INSUFFICIENT_BUFFER`, and `lpbytesreturned` is 0.
/// - If the output buffer is too small to hold all of the data but can hold some entries, some
///   drivers will return as much data as fits. In this case, the call fails, sets `ERROR_MORE_DATA`,
///   and `lpbytesreturned` indicates the amount of data received. Your application should call
///   `DeviceIoControl` again with the same operation, specifying a new starting point.
///
/// # Arguments
/// - `[in] hdevice`: A handle to the device on which the operation is to be performed. The
///   device is typically a volume, directory, file, or stream. To retrieve a device handle,
///   use [`CreateFile2`]
/// - `[in] dwiocontrolcode`: The control code for the operation. This value identifies the
///   specific operation to be performed and the type of device on which to perform it.
/// - `[in, optional] lpinbuffer`: A pointer to the input buffer that contains the data required
///   to perform the operation. The format of this data depends on the value of the `dwiocontrolcode`
///   parameter. This parameter can be NULL if `dwiocontrolcode` specifies an operation that does not
///   require input data.
/// - `[in] ninbuffersize`: The size of the input buffer, in bytes.
/// - `[out, optional] lpoutbuffer`: A pointer to the output buffer that is to receive the data
///   returned by the operation. The format of this data depends on the value of the `dwiocontrolcode`
///   parameter. This parameter can be NULL if `dwiocontrolcode` specifies an operation that does
///   not return data.
/// - `[in] noutbuffersize`: The size of the output buffer, in bytes.
/// - `[out, optional] lpbytesreturned`: A pointer to a variable that receives the size of the data
///   stored in the output buffer, in bytes.
/// - `[in, out, optional] lpOverlapped`: A pointer to an OVERLAPPED structure. Currently not supported
///   must be NULL
/// 
/// # TODO: #117 Mock reading audio data
///
/// [docs_DeviceIoControl]: https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-deviceiocontrol
/// [docs_IOCTL_CDROM_READ_TOC_EX]: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_read_toc_ex
/// [docs_IOCTL_STORAGE_GET_DEVICE_NUMBER]: https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-ioctl_storage_get_device_number
/// [docs_CDROM_TOC]: https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ns-ntddcdrm-_cdrom_read_toc_ex
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
    // Adding safety checks as assertions in mock programatically ensures all tested callsites
    // conform to requirements.
    // Cannot check validity of HANDLE.
    // Cannot check size_of lp*buffer - unable to reliably identify this due to opaque c_void
    match dwiocontrolcode as i32 {
        IOCTL_CDROM_READ_TOC_EX => {
            assert_eq!(
                ninbuffersize.strict_cast::<usize>(),
                size_of::<CDROM_READ_TOC_EX>()
            );
            assert_eq!(
                noutbuffersize.strict_cast::<usize>(),
                size_of::<CDROM_TOC>()
            );
        }
        IOCTL_STORAGE_GET_DEVICE_NUMBER => {
            assert!(lpinbuffer.is_null());
            assert_eq!(ninbuffersize, 0);
            assert_eq!(
                noutbuffersize.strict_cast::<usize>(),
                size_of::<STORAGE_DEVICE_NUMBER>()
            );
        }
        _ => unimplemented!("unsupported control code"),
    };
    assert!(!lpbytesreturned.is_null());
    assert!(lpoverlapped.is_null());

    match dwiocontrolcode.strict_cast::<i32>() {
        IOCTL_CDROM_READ_TOC_EX => {
            let toc = match hdevice {
                DEFINITELY_MAYBE => CDROM_TOC::from(DefinitelyMaybe),
                THE_WALL_1 => CDROM_TOC::from(TheWallDisc1),
                THE_WALL_2 => CDROM_TOC::from(TheWallDisc2),
                BAT_OUT_OF_HELL_REVAMPED => CDROM_TOC::from(BatOutOfHellRevamped),
                THE_BOY_WHO_KNEW_TOO_MUCH => CDROM_TOC::from(TheBoyWhoKnewTooMuch),
                _ => panic!("unknown album"),
            };
            assert_eq!(noutbuffersize as usize, size_of_val(&toc));
            unsafe { *(lpoutbuffer as *mut CDROM_TOC) = toc };
            unsafe { lpbytesreturned.write(toc.size().strict_cast()) };
        }
        IOCTL_STORAGE_GET_DEVICE_NUMBER => {
            let info = STORAGE_DEVICE_NUMBER {
                DeviceType: const { FILE_DEVICE_CD_ROM.strict_cast_unsigned() },
                DeviceNumber: hdevice as u32,
                PartitionNumber: -1_i32 as u32,
            };
            assert_eq!(noutbuffersize.strict_cast::<usize>(), size_of_val(&info));
            unsafe { *(lpoutbuffer as *mut STORAGE_DEVICE_NUMBER) = info };
            unsafe { *lpbytesreturned = size_of_val(&info).strict_cast() };
        }
        _ => todo!("mock DeviceIoControl for additional control codes"),
    }
    success!()
}

/// Duplicates an object handle. (See also [MS learn][docs])
///
/// # SAFETY
/// Safety remarks & mock only provided for use with file handles within a single process.
///
/// - `hsourcehandle` must be a valid pointer to a `HANDLE` as defined at [`std::ptr::read`]
/// - `lpTargetHandle` does not point to a valid open `HANDLE`, as this handle will be lost and
///   will not be closed. The mock enforces this by checking that `lpTargetHandle` points to an
///   `INVALID_HANDLE_VALUE`.
/// - The returned handle must be properly closed via [CloseHandle] when no longer required.
/// - The pointer to the returned handle (`lpTargetHandle`) must not be `null` as the handle will
///   duplicated but not returned. You will lose system resources until the target process terminates.
///
/// # Notes
/// Supported use case is restricted to duplicating handles:
/// - owned by the current thread
/// - to be used by the current
/// - which cannot be inherited by spawned child processes
/// - with `dwOptions` = `DUPLICATE_SAME_ACCESS`
///
/// # Returns / `last_os_error`
/// If the function succeeds, the return value is nonzero. If the function fails, the return value
/// is zero. To get extended error information, call `GetLastError`.
///
/// # Arguments
/// - `[in] hSourceProcessHandle`: A handle to the process with the handle to be duplicated. It is
///   recommended to use [`GetCurrentProcess`] to obtain a handle with suitable access rights which
///   references the current process. Other use cases are not supported / documented.
/// - `[in] hSourceHandle`: The handle to be duplicated. This is an open object handle that is valid
///   in the context of the source process.
/// - `[out] lpTargetHandle`: A pointer to a variable that receives the duplicate handle. This
///   handle value is valid in the context of the target process.
/// - `[in] dwDesiredAccess`: The access requested for the new handle. For the flags that can be
///   specified for each object type, see [MS-Learn][docs-remarks]. This parameter is ignored if the
///   dwOptions parameter specifies the `DUPLICATE_SAME_ACCESS` flag. Currently, only using
///   `DUPLICATE_SAME_ACCESS` is supported and `0` must be passed to `dwDesiredAccess`.
/// - `[in] bInheritHandle`: Whether the handle is inheritable. If `TRUE`, the duplicate handle
///   can be inherited by new processes created by the target process. If `FALSE`, the new handle
///   cannot be inherited. Currently only `FALSE` is supported.
/// - `[in] dwOptions`: Optional actions. This parameter can be zero, or any combination of the
///   following values:
///   - `DUPLICATE_CLOSE_SOURCE`: Closes the source handle. This occurs regardless of any error
///     status returned. Currently not supported.
///   - `DUPLICATE_SAME_ACCESS`: Ignores the `dwDesiredAccess` parameter. The duplicate handle has
///     the same access as the source handle. Currently this value must be passed to `dwOptions`.
///
/// [docs]: https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle
/// [docs-remarks]: https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle#remarks
pub unsafe fn DuplicateHandle(
    hsourceprocesshandle: HANDLE,
    hsourcehandle: HANDLE,
    htargetprocesshandle: HANDLE,
    lptargethandle: *mut HANDLE,
    dwdesiredaccess: u32,
    binherithandle: BOOL,
    dwoptions: u32,
) -> BOOL {
    assert!(!hsourceprocesshandle.is_null());
    assert!(hsourceprocesshandle.is_aligned());
    assert_eq!(
        hsourceprocesshandle,
        unsafe { GetCurrentProcess() },
        "source process must be current process"
    );
    assert!(!hsourcehandle.is_null());
    assert!(hsourcehandle.is_aligned());
    assert_ne!(hsourcehandle, INVALID_HANDLE_VALUE, "invalid source handle");
    assert!(!htargetprocesshandle.is_null());
    assert!(htargetprocesshandle.is_aligned());
    assert_eq!(
        htargetprocesshandle,
        unsafe { GetCurrentProcess() },
        "target process must be current process"
    );
    assert!(!lptargethandle.is_null());
    assert!(lptargethandle.is_aligned());
    assert_eq!(
        unsafe { lptargethandle.read() },
        INVALID_HANDLE_VALUE,
        "target handle must not be a valid handle"
    );
    assert_eq!(dwdesiredaccess, 0);
    assert_eq!(binherithandle, FALSE);
    assert_eq!(
        dwoptions,
        const { DUPLICATE_SAME_ACCESS.strict_cast::<u32>() }
    );
    unsafe { lptargethandle.write(hsourcehandle) };
    success!()
}

/// Retrieves a pseudo handle for the current process.
///
/// A pseudo handle is a special constant that is interpreted as the current process handle.
/// For compatibility with future operating systems, it is best to call `GetCurrentProcess` instead
/// of hard-coding this constant value. The calling process can use a pseudo handle to specify its
/// own process whenever a process handle is required. Pseudo handles are not inherited by child
/// processes. This handle has the `PROCESS_ALL_ACCESS` access right to the process object.
///
/// # SAFETY
/// No particular safety invariants must be upheld. `unsafe` is simply applyed by default to the
/// generated ffi binding.
///
/// # Note
/// - The pseudo handle *need not be closed* when it is no longer needed. Calling the `CloseHandle`
///   function with a pseudo handle has no effect.
/// - If the pseudo handle is duplicated by `DuplicateHandle`, the duplicate handle must be closed.
///   Duplication is currently forbidden by the mock `DuplicateHandle`.
pub unsafe fn GetCurrentProcess() -> HANDLE {
    -1 as _
}

/// Enumerates the device interfaces that are contained in a device information set.
///
/// Repeated calls to this function return an [`SP_DEVICE_INTERFACE_DATA`] structure for a different
/// device interface. This function can be called repeatedly to get information about interfaces
/// in a device information set that are associated with a particular device information element
/// or that are associated with all device information elements.
///
/// # Usage
///
/// Repeatedly increment `memberindex` and retrieve an interface until this function fails and
/// [`last_os_error`][std::io::Error::last_os_error] returns
/// [`ERROR_NO_MORE_ITEMS`][ERROR_NO_MORE_ITEMS].
///
/// # Return value / `last_os_error`
/// - non-zero: success
/// - 0, ERROR_NO_MORE_ITEMS: member index out of bounds
///
/// # SAFETY
/// - The caller must set `deviceinterfacedata.cbSize` to `size_of::<SP_DEVICE_INTERFACE_DATA>()`
///
/// # Note
/// If `deviceinfodata` specifies a  particular device, the `memberindex` is relative to only the
/// interfaces exposed by that device.
///
/// # Arguments
/// - `[in] deviceinfoset`: A pointer to a device information set that contains the device
///   interfaces for which to return information. This handle is typically returned by
///   [SetupDiGetClassDevsW].
/// - `[in, optional] deviceinfodata`: A pointer to an `SP_DEVINFO_DATA` structure that specifies a
///   device information element in `deviceinfoset`. This parameter is optional and can be NULL.
///     - If this parameter is specified, `SetupDiEnumDeviceInterfaces` constrains the enumeration
///       to the interfaces that are supported by the specified device. (But see "Note", above)
///     - If this parameter is NULL, repeated calls to `SetupDiEnumDeviceInterfaces` return
///       information about the interfaces that are associated with all the device information
///       elements in `deviceinfoset`
///     - This pointer is typically returned by `SetupDiEnumDeviceInfo`
///     - **Usage not supported in mock**
/// - `[in] interfaceclassguid`: A pointer to a GUID that specifies the device interface class
///   for the requested interface.
/// - `[in] memberindex`: A zero-based index into the list of interfaces in the device
///   information set. See `Usage`, above for details.
/// - `[out] deviceinterfacedata`: A pointer to a caller-allocated buffer that contains, on
///   successful return, a completed [`SP_DEVICE_INTERFACE_DATA`] structure that identifies an
///   interface that meets the search parameters.
pub unsafe fn SetupDiEnumDeviceInterfaces(
    deviceinfoset: HDEVINFO,
    #[expect(unused_variables, reason = "irrelevant for mock")]
    deviceinfodata: *const SP_DEVINFO_DATA,
    #[expect(unused_variables, reason = "irrelevant for mock")] interfaceclassguid: *const GUID,
    memberindex: u32,
    deviceinterfacedata: *mut SP_DEVICE_INTERFACE_DATA,
) -> BOOL {
    let album = match (deviceinfoset, memberindex) {
        (ALL_ALBUMS, 0) => DefinitelyMaybe as u32,
        (ALL_ALBUMS, 1) => TheWallDisc1 as u32,
        (ALL_ALBUMS, 2) => TheWallDisc2 as u32,
        (ALL_ALBUMS, _) => failure!(ERROR_NO_MORE_ITEMS),
        _ => panic!("unknown deviceinfoset"),
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
    success!()
}

/// The SetupDiGetClassDevs function returns a handle to a device information set that contains
/// requested device information elements for a local computer.
///
/// See: https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetclassdevsw
///
/// # SAFETY
/// - The caller of SetupDiGetClassDevs must delete the returned device information set when it is
///   no longer needed by calling SetupDiDestroyDeviceInfoList.
/// - It is recommended to avoid direct calls to SetupDiGetClassDevsW and use associated functions
///   on DeviceInfoSet which provides a type-safe wrapper for the returned handle and calls
///   SetupDiDestroyDeviceInfoList when dropped.
pub unsafe fn SetupDiGetClassDevsW(
    classguid: *const GUID,
    #[expect(unused_variables, reason = "irrelevant for mock")] enumerator: PCWSTR,
    #[expect(unused_variables, reason = "irrelevant for mock")] hwndparent: HWND,
    #[expect(unused_variables, reason = "irrelevant for mock")] flags: u32,
) -> HDEVINFO {
    let classguid = Guid(unsafe { *classguid });
    match classguid {
        guid if guid == Guid(GUID_DEVINTERFACE_CDROM) => ALL_ALBUMS,
        _ => todo!("unknown guid"),
    }
}

/// Used in one of two ways. Usually in sequence:
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
    #[expect(unused_variables, reason = "irrelevant for mock")]
    deviceinfodata: *mut SP_DEVINFO_DATA,
) -> BOOL {
    assert!(!deviceinterfacedata.is_null());
    let album = match (
        deviceinfoset,
        // SAFETY: Not-null
        unsafe { *deviceinterfacedata }.InterfaceClassGuid.data1,
    ) {
        (ALL_ALBUMS, id) if id == DefinitelyMaybe as u32 => DefinitelyMaybe,
        (ALL_ALBUMS, id) if id == TheWallDisc1 as u32 => TheWallDisc1,
        (ALL_ALBUMS, id) if id == TheWallDisc2 as u32 => TheWallDisc2,
        _ => panic!("unrecognised call to SetupDiGetDeviceInterfaceDetailW"),
    };
    let album_path = format!(r"\\.\{}", album.assets_path().display());
    let path_len = album_path.len();
    match (
        deviceinterfacedetaildata.is_null(),
        deviceinterfacedetaildatasize,
        requiredsize.is_null(),
    ) {
        (true, 0, false) => {
            // This potentially slightly oversizes the requirement. This is acceptable for
            // testing purposes to avoid the complexity of calculating the size required when
            // *replacing* the default buffer of [u16; 1] with a sufficiently large buffer.
            let size = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() + path_len;
            // SAFETY:
            // - validated pointer is not NULL (in match arm)
            // - mock runs on debug build so as u32 will panic on overflow.
            unsafe { *requiredsize = size as u32 };
            failure!(ERROR_INSUFFICIENT_BUFFER)
        }
        (false, _, true) => {
            // SAFETY check
            DeviceDetails::check_size(path_len as u32)
                .expect("DeviceDetails has space for MAX_PATH_CHARS, we should be below that");
            let mut data = DeviceDetails::default();
            match data.set_path(&WinString::from(album_path)) {
                Ok(words) if words == path_len + 1 => {
                    // SAFETY:
                    // - validated pointer is not NULL (in match arm)
                    // - DeviceDetails is directly compatible with SP_DEVICE_INTERFACE_DETAIL_DATA_W
                    // - DeviceDetails has space for MAX_PATH_CHARS
                    unsafe { *(deviceinterfacedetaildata as *mut DeviceDetails) = data };
                    success!()
                }
                _ => panic!("setting path failed"),
            };
        }
        _ => panic!("invalid call to SetupDiGetDeviceInterfaceDetailW"),
    }
}
