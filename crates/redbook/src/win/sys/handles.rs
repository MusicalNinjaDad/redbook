use std::{
    fmt::Display,
    io::{self, ErrorKind},
    mem::offset_of,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

use tracing::field::Empty;
use tracing_result::Trace;

use crate::{
    Device, FRAME_SIZE, Frame,
    win::{
        MAX_PATH_CHARS,
        convert::{Guid, Sector, WinPath, WinString},
    },
};

use super::{
    BOOL, CDDA, CDROM_READ_TOC_EX, CDROM_TOC, CloseHandle, CreateFile2, DIGCF_DEVICEINTERFACE,
    DIGCF_PRESENT, DUPLICATE_SAME_ACCESS, DeviceIoControl, DuplicateHandle,
    ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, FALSE, FILE_DEVICE_CD_ROM, FILE_SHARE_READ,
    GENERIC_READ, GetCurrentProcess, HANDLE, HDEVINFO, INVALID_HANDLE_VALUE, IOCTL_CDROM_RAW_READ,
    IOCTL_CDROM_READ_TOC_EX, IOCTL_STORAGE_GET_DEVICE_NUMBER, OPEN_EXISTING, RAW_READ_INFO,
    SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, STORAGE_DEVICE_NUMBER,
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
    SetupDiGetDeviceInterfaceDetailW,
};

pub use safety::DriveHandle;
use safety::IntoDriveHandle;

#[cfg(test)]
use crate::test_fixtures::albums::TestAlbum;
/// Contains and seals the only way to create a DriveHandle, thus enforcing invariants.
mod safety {
    use super::*;
    /// A open file handle which can only be created by calling
    /// [`into_drive_handle`][HANDLE::into_drive_handle], thus ensuring invariants are upheld.
    ///
    /// # SAFETY guarantees provided
    /// - Stored handle is not publicly accessible.
    /// - Handle is closed on Drop.
    /// - Can only be publically created via [open][Self::open] which validates the handle.
    ///
    /// # TODO
    /// - Consider how https://learn.microsoft.com/en-us/windows/win32/api/handleapi/nf-handleapi-duplicatehandle
    ///   could help provide Clone, etc.
    #[derive(Debug, PartialEq, Eq)]
    pub struct DriveHandle(HANDLE);

    impl DriveHandle {
        /// Allows (only) `super` to implement safe wrappers around ffi calls which require the
        /// underlying [`HANDLE`]
        ///
        /// # SAFETY
        ///
        /// Callers must ensure that the raw handle is not:
        /// - cloned
        /// - copied
        /// - closed
        /// - shared between threads
        #[expect(unsafe_code, reason = "handle must only used for passing to ffi")]
        pub(super) unsafe fn handle(&self) -> &HANDLE {
            &self.0
        }
    }

    trait Sealed {}
    impl Sealed for HANDLE {}

    #[expect(private_bounds, reason = "sealed for safety")]
    pub(super) trait IntoDriveHandle: Sealed {
        fn into_drive_handle(self) -> io::Result<DriveHandle>;
    }

    impl IntoDriveHandle for HANDLE {
        #[inline]
        fn into_drive_handle(self) -> io::Result<DriveHandle> {
            (self != INVALID_HANDLE_VALUE)
                .ok_or_else(io::Error::last_os_error)
                .or_warn("invalid handle")?;

            let drivehandle = DriveHandle(self);
            (drivehandle
                .device_number()
                .or_warn("unable to identify device type")?
                .DeviceType
                == const { FILE_DEVICE_CD_ROM.strict_cast::<u32>() })
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::Unsupported,
                    "Device does not support CDROM commands",
                )
            })
            .or_warn("unsupported device")?;

            Ok(drivehandle)
        }
    }

    #[cfg(test)]
    /// These tests must be here as they require instantiating an invalid DriveHandle which is not
    /// possible outside `mod safety`
    mod miri {
        use super::*;

        #[test]
        #[should_panic(expected = "invalid source handle")]
        /// This should panic during cloning, before validation in into_drive_handle
        fn clone_invalid() {
            let invalid = DriveHandle(INVALID_HANDLE_VALUE);
            let _cloned = invalid.clone();
        }
    }
}

/// # SAFETY
/// - `DriveHandle`s are safe to [`Send`] to other threads as only one thread can ever access the
///   underlying device at any time.
#[expect(
    unsafe_code,
    reason = "Want to be able to rip in one thread and encode in another"
)]
// SAFETY: See documentation comment
unsafe impl Send for DriveHandle {}

// TODO: FIX SAFETY requirements of as_handle, as_handle_mut, CreateFile2
impl Drop for DriveHandle {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "ffi call")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "inline call to get handle"
        )]
        unsafe {
            // SAFETY: handle
            // - was opened and validated in `open()`
            // - has not been closed (no such methods provided on Self)
            // - has not been externally mutated (no such methods provided on Self)
            CloseHandle(*self.handle() as *mut _);
        }
        // Not checking for success: cannot meaningfully handle CloseHandle failure during drop
    }
}

/// Cloning a DriveHandle calls `DuplicateHandle` in background and provides a *new handle* to the
/// same underlying drive, which can be used within the current process, but cannot be passed to
/// a child process. (Please, raise an issue / PR if you need to pass a DriveHandle to a spawned
/// process).
///
/// **IMPORTANT:** See the note regarding equality below.
///
/// # Note
///
/// - Contrary to the usual expectations, the cloned handle will *not compare equal* to the original.
///   Code should not, and unsafe code MUST NOT, rely on assuming `drivehandle.clone() == drivehandle`
///   as this will not be true!
/// - The cloned handle must be independently closed (ideally automatically via the [`Drop`][Self::drop]
///   implemenation).
/// - Cloning a `DriveHandle` *may panic*, if for some reason the validation of the new handle fails.
///   This could occur for example, if the underlying drive is unplugged before cloning.
///
/// # TODO
/// - TODO: #111 Provide DriveHandle::duplicate -> io::Result<DriveHandle>
impl Clone for DriveHandle {
    fn clone(&self) -> Self {
        let mut cloned = INVALID_HANDLE_VALUE;

        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "inlined calls to GetCurrentProcess, self.handle, dereferencing self.handle"
        )]
        unsafe {
            // SAFETY: inline
            DuplicateHandle(
                GetCurrentProcess(),
                // type wrapper ensures validity
                *self.handle(),
                GetCurrentProcess(),
                // instantiated above as INVALID_HANDLE_VALUE,
                // will be immediately wrapped in a DriveHandle, which calls CloseHandle on Drop
                &mut cloned as *mut HANDLE,
                0,
                FALSE,
                const { DUPLICATE_SAME_ACCESS.strict_cast() },
            )
        };
        cloned
            .into_drive_handle()
            .expect("drive is no longer available")
    }
}

impl DriveHandle {
    pub(super) fn device_number(&self) -> io::Result<STORAGE_DEVICE_NUMBER> {
        let mut device = STORAGE_DEVICE_NUMBER::default();
        let mut bytes_returned = 0;

        #[expect(unsafe_code, reason = "ffi call to get device type")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "inline call to get handle"
        )]
        unsafe {
            // SAFETY: inline based on
            // - https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ni-winioctl-ioctl_storage_get_device_number
            // - https://learn.microsoft.com/en-us/windows/win32/api/ioapiset/nf-ioapiset-deviceiocontrol
            DeviceIoControl(
                // known valid handle for a storage device, opened and checked above
                *self.handle(),
                // control code (bindgen creates i32, function requires u32)
                const { IOCTL_STORAGE_GET_DEVICE_NUMBER.strict_cast() },
                // null pointer - no input buffer
                null(),
                // 0 size - no input buffer
                0,
                // output is a STORAGE_DEVICE_NUMBER
                &mut device as *mut _ as *mut _,
                const { size_of::<STORAGE_DEVICE_NUMBER>().strict_cast() },
                &mut bytes_returned,
                // optional - not using overlapped operations
                null_mut(),
            )
        }
        .validate(
            Some(const { size_of::<STORAGE_DEVICE_NUMBER>().strict_cast() }),
            bytes_returned,
        )?;
        Ok(device)
    }

    /// Read the Table of Contents from the disc.
    ///
    /// # Errors
    ///
    /// Notably, this can return OS error 21 (device not ready) = no disc in drive.
    ///
    /// TODO: consider making public if we make win::sys::cdrom_toc public too
    fn cdrom_toc(&self) -> io::Result<CDROM_TOC> {
        let _error_span = tracing::error_span!("DriveHandle::cdrom_toc").entered();

        // SAFETY: We rely on `size_of::<>()`. Changing type requires updating the
        // call to `DeviceIoControl` below
        let toc_command: CDROM_READ_TOC_EX = CDROM_READ_TOC_EX {
            SessionTrack: 1,
            ..Default::default()
        };

        // SAFETY: We rely on `size_of::<>()`. Changing type requires updating the
        // call to `DeviceIoControl` below
        let mut toc: CDROM_TOC = CDROM_TOC::default();
        let mut bytes_read: u32 = 0;

        #[expect(unsafe_code, reason = "ffi call to read toc")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "inline call to get handle"
        )]
        unsafe {
            // SAFETY: inline based on
            // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_read_toc_ex
            DeviceIoControl(
                // handle is valid for a CD_ROM_DRIVE. The only public way to construct a
                // DeviceHandle includes validation
                *self.handle(),
                const { IOCTL_CDROM_READ_TOC_EX.strict_cast_unsigned() },
                // points to a buffer of type CDROM_READ_TOC_EX
                &toc_command as *const _ as *const _,
                // indicates the size, in bytes, of the input buffer,
                // which must be >= sizeof(CDROM_READ_TOC_EX).
                const { size_of::<CDROM_READ_TOC_EX>().strict_cast() },
                // CDROM_READ_TOC_EX does not allow setting `Format` but
                // `CDROM_READ_TOC_EX_FORMAT_TOC` is `0` (default) whereby
                // The output data is reported in a CDROM_TOC structure.
                &mut toc as *mut _ as *mut _,
                const { size_of::<CDROM_TOC>().strict_cast() },
                &mut bytes_read as *mut _,
                null_mut(),
            )
        }
        .validate(Some(toc.size().strict_cast()), bytes_read)?;
        Ok(toc)
    }
}

impl Device for DriveHandle {
    fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = PathBuf::from(path.as_ref());
        let _debug = tracing::debug_span!("DriveHandle::open", path = %path.display()).entered();

        let windrive = WinPath::from(path).into_dos_device_path();
        tracing::debug!(%windrive);

        #[expect(unsafe_code, reason = "ffi call")]
        let handle = unsafe {
            // SAFETY:
            // - `path` is owned by this function and therefore valid for duration of this block
            // - returned handle will be stored in `Self` immediately after successful creation
            // - `Drop` calls `CloseHandle`
            CreateFile2(
                windrive.as_pcwstr(),
                GENERIC_READ,
                const { FILE_SHARE_READ.strict_cast_unsigned() },
                const { OPEN_EXISTING.strict_cast_unsigned() },
                null(),
            )
        }
        .into_drive_handle()
        .or_error("getting handle for drive")?;
        tracing::debug!(?handle, "opened successfully");

        Ok(handle)
    }

    fn fs_path(&self) -> io::Result<PathBuf> {
        let info = self.device_number()?;

        let path = PathBuf::from(format!(r"\\.\CDROM{}", info.DeviceNumber));

        Ok(path)
    }

    fn read_cdda(&self, start: Frame, buf: &mut [u8]) -> io::Result<Frame> {
        let span = tracing::error_span!(
            "DriveHandle::read_cdda",
            %start,
            buf_size = buf.len(),
            frames_to_read = Empty,
            start_offset_sector = Empty,
        )
        .entered();

        let bytes_to_read = u32::try_from(buf.len())
            .map_err(|_| io::Error::new(ErrorKind::OutOfMemory, "buffer too large"))
            .or_error("buffer too large")?;

        // SAFETY: buffer size must be >= sizeof(SectorCount * RAW_SECTOR_SIZE)
        // SectorCount = frames_to_read
        // RAW_SECTOR_SIZE = FRAME_SIZE
        let frames_to_read = bytes_to_read
            .checked_div_exact(const { FRAME_SIZE.strict_cast() })
            .ok_or_else(|| io::Error::new(ErrorKind::Unsupported, "invalid buffer size"))
            .or_error("buffer not sized for CDDA")?;

        span.record("frames_to_read", frames_to_read);

        let offset = Sector::from_frame(start).offset();
        span.record("start_offset_sector", offset);

        // SAFETY: We rely on `size_of::<>()`. Changing type requires updating the
        // call to `DeviceIoControl` below
        let read_command: RAW_READ_INFO = RAW_READ_INFO {
            DiskOffset: offset,
            SectorCount: frames_to_read,
            TrackMode: CDDA,
        };

        let mut bytes_read: u32 = 0;

        #[expect(unsafe_code, reason = "ffi call to read raw cdda data")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "inline call to get handle"
        )]
        // SAFETY: inline based on https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_raw_read
        unsafe {
            DeviceIoControl(
                // handle known valid thanks to type wrapper
                *self.handle(),
                // handle known to represent drive able to process CDROM controls, thanks to type wrapper
                const { IOCTL_CDROM_RAW_READ.strict_cast_unsigned() },
                // If the IOCTL is from user mode, Irp->AssociatedIrp.SystemBuffer contains a RAW_READ_INFO
                // structure that specifies the starting disk offset, the sector count, and the track mode
                // (XA or CDDA) for the read.
                &read_command as *const _ as *const _,
                // Parameters.DeviceIoControl.InputBufferLength specifies the size, in bytes, of the
                // structure, which must be >= sizeof(RAW_READ_INFO)
                const { size_of::<RAW_READ_INFO>().strict_cast() },
                // Buffer correctly sized, validated above
                buf as *mut _ as *mut _,
                // SAFETY:
                // 1. bytes_to_read calculated from buf.len() is validated at runtime, above
                // 2. This must be >= sizeof(SectorCount * RAW_SECTOR_SIZE)
                //    - SectorCount calculated as bytes_to_read.checked_exact_div(FRAME_SIZE)
                bytes_to_read,
                &mut bytes_read as *mut _,
                null_mut(),
            )
        }
        .validate(Some(bytes_to_read), bytes_read)
        .or_warn("raw CDDA data read failed")?;

        Ok(Frame::new(frames_to_read.strict_cast()))
    }

    fn read_toc(&self) -> io::Result<crate::TocEntries> {
        let _debug = tracing::debug_span!("DriveHandle::read_toc").entered();
        Ok(self.cdrom_toc()?.into())
    }

    fn raw_toc(&self) -> io::Result<Vec<u8>> {
        Ok(self.cdrom_toc()?.as_raw_bytes())
    }
}

trait IoControl {
    fn validate(&self, bytes_expected: Option<u32>, bytes_returned: u32) -> io::Result<()>;
}

impl IoControl for BOOL {
    fn validate(&self, bytes_expected: Option<u32>, bytes_returned: u32) -> io::Result<()> {
        let _span = tracing::error_span!("DeviceIoControl::validate", return_code = self, ?bytes_expected, %bytes_returned).entered();

        (*self != 0)
            .ok_or_else(io::Error::last_os_error)
            .or_warn("error calling DeviceIoControl")?;

        if let Some(bytes_expected) = bytes_expected {
            (bytes_returned == bytes_expected)
                .ok_or_else(|| {
                    io::Error::new(
                        ErrorKind::UnexpectedEof,
                        format!("expected {bytes_expected} bytes, received {bytes_returned}"),
                    )
                })
                .or_warn("error calling DeviceIoControl")?;
        };

        Ok(())
    }
}

#[cfg(test)]
impl TryFrom<&DriveHandle> for TestAlbum {
    type Error = io::Error;

    fn try_from(handle: &DriveHandle) -> Result<Self, Self::Error> {
        #[expect(unsafe_code, reason = "mock handle")]
        // SAFETY: mock handle
        let h = unsafe { *handle.handle() };
        h.try_into()
    }
}

#[derive(Debug)]
pub struct DeviceInfoSet {
    handle: HDEVINFO,
    class: Guid,
}

// TODO: Extend Safety requirements for SetupDiGetClassDevs
impl Drop for DeviceInfoSet {
    /// Ensures that list is correctly destroyed as per
    /// https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetclassdevsw#remarks
    ///
    /// Which requires that the caller of SetupDiGetClassDevs must delete the returned device
    /// information set when it is no longer needed by calling SetupDiDestroyDeviceInfoList.
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "ffi call")]
        // SAFETY: handle
        // - is of correct type, based on type-safe wrapper
        // - has not been closed (see safety sections for [as_handle_mut] & [as_handle])
        // - has not been externally mutated (see safety section for [as_handle_mut])
        unsafe {
            SetupDiDestroyDeviceInfoList(self.handle as *mut _);
        }
        // Not checking for success: cannot meaningfully handle CloseHandle failure during drop
    }
}

impl DeviceInfoSet {
    pub fn for_device_class(class: Guid) -> io::Result<Self> {
        let _tracing_span =
            tracing::error_span!("DeviceInfoSet::for_device_class", %class).entered();

        #[expect(unsafe_code, reason = "ffi call")]
        // SAFETY:
        // 1. SetupDiGetClassDevsW returns a handle to a device information set that contains all
        //    installed devices that matched the supplied parameters.
        // 2. If the operation fails, the function returns INVALID_HANDLE_VALUE.
        // 3. This is then immediately wrapped in a DeviceInfoSet which calls
        //    SetupDiDestroyDeviceInfoList on Drop
        let handle = unsafe {
            SetupDiGetClassDevsW(
                // A pointer to the GUID for a device setup class or a device interface class.
                &class.0 as *const _,
                // This pointer is optional and can be NULL.
                // If an enumeration value is not used to select devices, set Enumerator to NULL.
                // - No PnP enumeration required
                null(),
                // A handle to the top-level window to be used for a user interface that is
                // associated with installing a device instance in the device information set.
                // This handle is optional and can be NULL.
                // - Not installing a device
                null_mut(),
                // Filter the device information elements that are added to the device
                // information set. This parameter can be a bitwise OR of zero or more flags
                // - DIGCF_DEVICEINTERFACE: Return devices that support device interfaces for the
                //   specified device interface classes.
                // - DIGCF_PRESENT: Return only devices that are currently present in a system.
                DIGCF_DEVICEINTERFACE as u32 | DIGCF_PRESENT as u32,
            )
        };
        if handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error()).or_warn("invalid handle")?
        } else {
            Ok(Self { handle, class })
        }
    }

    /// Obtain the GUID of the associated device class
    pub fn class(&self) -> Guid {
        self.class
    }

    /// Get the detail data for a given member of the device information set
    pub fn get_member(&self, index: usize) -> io::Result<Option<DeviceDetails>> {
        // This is an wonderful, 3-step process in the C++ API so we expose it as a single pub fn
        // and wrap each step in a private function to try and preserve our, and your, sanity.

        let interface_data = match self.get_interface_data(index.strict_cast())? {
            Some(data) => data,
            None => return Ok(None),
        };

        self.check_required_buffer_size(&interface_data)?;

        #[expect(unsafe_code, reason = "data must fit in a `DeviceDetails`")]
        // SAFETY: We just called check_required_buffer_size
        let member = unsafe { self.get_member_details(&interface_data)? };

        Ok(Some(member))
    }

    /// Provides the basic information required to be able to get detail data for a given member.
    ///
    /// Returns `Ok(None)` if the member does not exist.
    fn get_interface_data(&self, memberindex: u32) -> io::Result<Option<SP_DEVICE_INTERFACE_DATA>> {
        let _span =
            tracing::error_span!("DeviceInfoSet::get_interface_data", memberindex).entered();

        // SAFETY: The caller must set DeviceInterfaceData.cbSize to sizeof(SP_DEVICE_INTERFACE_DATA)
        // before calling SetupDiEnumDeviceInterfaces
        let mut deviceinterfacedata = SP_DEVICE_INTERFACE_DATA {
            cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
            ..Default::default()
        };

        #[expect(unsafe_code, reason = "ffi call")]
        // SAFETY: inline based on
        // https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdienumdeviceinterfaces
        let get_data = unsafe {
            SetupDiEnumDeviceInterfaces(
                // A pointer to a device information set that contains the device
                // interfaces for which to return information.
                //
                // SAFETY: Known to be a valid handle based on typed wrapper
                self.handle,
                // If this parameter is NULL, repeated calls to SetupDiEnumDeviceInterfaces return
                // information about the interfaces that are associated with *all* the device
                // information elements in DeviceInfoSet
                null(),
                // A pointer to a GUID that specifies the device interface class for the
                // requested interface.
                &self.class.0 as *const _,
                // A zero-based index into the list of interfaces in the device information set.
                memberindex,
                // A pointer to a caller-allocated buffer that contains, on successful return,
                // a completed SP_DEVICE_INTERFACE_DATA structure that identifies an interface
                // that meets the search parameters.
                // The caller must set DeviceInterfaceData.cbSize to sizeof(SP_DEVICE_INTERFACE_DATA)
                // before calling this function.
                //
                // SAFETY: **cbSize set upon construction**
                &mut deviceinterfacedata as *mut _,
            )
        };

        // "repeatedly increment MemberIndex and retrieve an interface until this function
        // fails and GetLastError returns ERROR_NO_MORE_ITEMS"
        let err = io::Error::last_os_error();
        match (get_data, err.raw_os_error()) {
            (0, Some(ERROR_NO_MORE_ITEMS)) => Ok(None),
            (0, _) => {
                tracing::error!(%err, "failed to get data about device");
                Err(err)
            }
            _ => {
                tracing::debug!(guid = %Guid(deviceinterfacedata.InterfaceClassGuid), "got data for device");
                Ok(Some(deviceinterfacedata))
            }
        }
    }

    /// Validates that a [`DeviceDetails`] struct is large enough to house the details for `member`
    fn check_required_buffer_size(&self, member: &SP_DEVICE_INTERFACE_DATA) -> io::Result<()> {
        let _span = tracing::error_span!("DeviceInfoSet::check_required_buffer_size").entered();
        let mut requiredsize: u32 = 0;

        #[expect(unsafe_code, reason = "ffi call")]
        // SAFETY: https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceinterfacedetailw
        //
        // 1. Get the required buffer size. Call SetupDiGetDeviceInterfaceDetail with a
        // NULLDeviceInterfaceDetailData pointer, a DeviceInterfaceDetailDataSize of zero,
        // and a valid RequiredSize variable. In response to such a call, this function returns
        // the required buffer size at RequiredSize and fails with GetLastError
        // returning ERROR_INSUFFICIENT_BUFFER.
        let get_required_buffer_size = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                self.handle,
                member,
                // a NULLDeviceInterfaceDetailData pointer
                null_mut(),
                // a DeviceInterfaceDetailDataSize of zero
                0,
                // a valid RequiredSize variable
                //
                // Receives the required size of the DeviceInterfaceDetailData buffer.
                // This size includes the size of the fixed part of the structure plus the number
                // of bytes required for the variable-length device path string.
                &mut requiredsize as *mut _,
                null_mut(),
            )
        };
        let err = io::Error::last_os_error();
        match (get_required_buffer_size, err.raw_os_error()) {
            (0, Some(ERROR_INSUFFICIENT_BUFFER)) => tracing::debug!(requiredsize),
            _ => {
                tracing::error!(%err, "reading required buffer size");
                return Err(err);
            }
        };

        DeviceDetails::check_size(requiredsize)
    }

    #[expect(
        unsafe_code,
        reason = "must validate that details will fit in a DeviceDetails"
    )]
    /// Get detail data for a given `member`.
    ///
    /// # SAFETY
    ///
    /// Caller must guarantee that the data will fit in a DeviceDetails struct. It is recommended
    /// to call [Self::check_required_buffer_size] in order to validate this requirement.
    unsafe fn get_member_details(
        &self,
        member: &SP_DEVICE_INTERFACE_DATA,
    ) -> io::Result<DeviceDetails> {
        let _span = tracing::error_span!("DeviceInfoSet::get_member_details").entered();

        // SAFETY:
        // 1. cbSize is fixed to correct value via construction:
        //    the caller must set DeviceInterfaceDetailData.cbSize to
        //    sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA) before calling SetupDiGetDeviceInterfaceDetailW.
        //    The cbSize member always contains the size of the fixed part of the data structure,
        //    not a size reflecting the variable-length string at the end.
        //
        // 2. Caller must guarantee that struct large enough via call to DeviceDetails::check_size
        let mut deviceinterfacedetaildata = DeviceDetails::default();

        #[expect(unsafe_code, reason = "ffi call")]
        // SAFETY: https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceinterfacedetailw
        let get_details = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                // A pointer to a device information set that contains the device
                // interfaces for which to return information.
                self.handle,
                // A pointer to an SP_DEVICE_INTERFACE_DATA structure that specifies the interface
                // in DeviceInfoSet for which to retrieve details. A pointer of this type is
                // typically returned by SetupDiEnumDeviceInterfaces.
                member,
                // A pointer to an SP_DEVICE_INTERFACE_DETAIL_DATA structure to receive information
                // about the specified interface.
                //
                // ** If this parameter is specified, the caller must set
                // DeviceInterfaceDetailData.cbSize to sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA)
                // before calling this function. The cbSize member always contains the size of the
                // fixed part of the data structure, not a size reflecting the variable-length
                // string at the end.**
                //
                // SAFETY:
                // 1. cbSize set upon construction
                // 2. buffer size validated by caller
                // 3. safe to cast to *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W as DeviceDetails defined
                //    with identical fields, layout & alignment
                &mut deviceinterfacedetaildata as *mut DeviceDetails
                    as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
                // The size of the DeviceInterfaceDetailData buffer. The buffer must be at least
                // (offsetof(SP_DEVICE_INTERFACE_DETAIL_DATA, DevicePath) + sizeof(TCHAR)) bytes,
                // to contain the fixed part of the structure and a single NULL to terminate an
                // empty MULTI_SZ string.
                //
                // Therefore this size must be the total size of the DeviceDetails struct, which
                // is known to have sufficient buffer for the fixed part, path + termination
                const { size_of::<DeviceDetails>() as u32 },
                null_mut(),
                null_mut(),
            )
        };

        match get_details {
            0 => {
                let err = io::Error::last_os_error();
                tracing::error!(get_details, %err);
                Err(err)
            }
            _ => Ok(deviceinterfacedetaildata),
        }
    }
}

#[repr(C)]
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
#[expect(nonstandard_style, reason = "mimic C++ struct")]
/// A custom variant of [SP_DEVICE_INTERFACE_DETAIL_DATA_W] with a pre-allocated buffer
/// large enough for any valid drive path (win32 MAX_PATH = 260 char)
///
/// [check_size][Self::check_size] is provided to allow for validation to avoid buffer overruns.
///
/// A real example of such a path is:
/// `\\?\usbstor#cdrom&ven_hl-dt-st&prod_dvdram_gue1n&rev_as00#4b4d444642414d3130353920&0#{53f56308-b6bf-11d0-94f2-00a0c91efb8b}\0`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct DeviceDetails {
    cbSize: u32 = const {size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32},
    DevicePath: [u16; MAX_PATH_CHARS] = [0; _],
}

#[repr(C, packed(1))]
#[cfg(target_arch = "x86")]
#[expect(nonstandard_style, reason = "mimic C++ struct")]
/// A custom variant of [SP_DEVICE_INTERFACE_DETAIL_DATA_W] with a pre-allocated buffer
/// large enough for any valid drive path (win32 MAX_PATH = 260 char)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DeviceDetails {
    cbSize: u32 = const {size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32},
    DevicePath: [u16; MAX_PATH_CHARS] = [0; _],
}

const _: () = {
    assert!(align_of::<DeviceDetails>() == align_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>());
    assert!(
        offset_of!(DeviceDetails, DevicePath)
            == offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath)
    );
};

impl DeviceDetails {
    /// Validate that the buffer provided by `DeviceDetails` is sufficient.
    ///
    /// It is recommended to first call `SetupDiGetDeviceInterfaceDetailW` as per C++ docs to
    /// get the required size, then to call `check_size` before using DeviceDetails to store the
    /// information provided by a second call to `SetupDiGetDeviceInterfaceDetailW`
    pub fn check_size(requiredsize: u32) -> io::Result<()> {
        (requiredsize <= size_of::<Self>() as u32)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "device path too long"))
    }

    /// This path is valid across reboots and valid to pass directly to [`CreateFile2`]
    pub fn path(&self) -> PathBuf {
        let path = WinString::from(self.DevicePath.as_slice());
        PathBuf::from(path.to_string())
    }

    /// Set the path. Returns the number of u16 "words" stored on success, or
    /// `ErrorKind::InvalidData` if the provided path is too large.
    #[cfg_attr(
        all(windows, not(any(test, doc))),
        expect(
            dead_code,
            reason = "only used in mock for SetupDiGetDeviceInterfaceDetailW"
        )
    )]
    pub(super) fn set_path(&mut self, path: &WinString) -> io::Result<usize> {
        let len = path.as_words().len();
        (len < self.DevicePath.len()).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "path too long: can only store {MAX_PATH_CHARS} words, received {len} words"
                ),
            )
        })?;
        self.DevicePath[..len].copy_from_slice(path.as_words());
        Ok(len)
    }
}

impl Display for DeviceDetails {
    /// Output the path, parsing correctly as null-terminated utf16
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self
            .DevicePath
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(self.DevicePath.len());
        Display::fmt(&String::from_utf16_lossy(&self.DevicePath[..len]), f)
    }
}

#[cfg(test)]
mod miri {
    use super::*;

    use crate::test_fixtures::albums::TestAlbum;

    #[test]
    fn clone() {
        let handle = HANDLE::from(TestAlbum::DefinitelyMaybe)
            .into_drive_handle()
            .unwrap();
        let cloned = handle.clone();

        // The mock for DuplicateHandle simply duplicates the handle value.
        // This assertion would not hold true with a real handle.
        assert_eq!(handle, cloned);
    }

    #[test]
    fn invalid() {
        assert!(INVALID_HANDLE_VALUE.into_drive_handle().is_err());
    }
}
