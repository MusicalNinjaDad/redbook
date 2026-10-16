use std::ops::{FromResidual, Residual, Try};

use crate::win::bindings::{
    FILE_DEVICE_CD_ROM, IOCTL_STORAGE_GET_DEVICE_NUMBER, SetupDiDestroyDeviceInfoList,
};

use super::*;
/// A open file handle which is known to point to a valid drive.
///
/// # SAFETY
/// - Stored handle is not public. Can only be created via provided functions ensuring this
///   is a safe new-type wrapper
/// - Handle is closed on Drop
#[derive(Debug, PartialEq, Eq)]
pub struct DriveHandle(HANDLE);

/// # SAFETY
/// - The only way to get the underlying [`HANDLE`] is via `unsafe` call to
///   [`as_handle`][Self::as_handle] which includes specific safety restrictions allowing
///   `DriveHandle` to be [Send]
/// - Not Sync as we have not enabled overlapped I/O or any internal sync mechanism
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
        unsafe {
            // SAFETY: handle
            // - was opened and validated in `open()`
            // - has not been closed (no such methods provided on Self)
            // - has not been externally mutated (no such methods provided on Self)
            CloseHandle(self.0 as *mut _);
        }
        // Not checking for success: cannot meaningfully handle CloseHandle failure during drop
    }
}

impl DriveHandle {
    pub fn open(path: WinString) -> io::Result<Self> {
        let _debug = tracing::debug_span!("opening drive handle", %path).entered();

        #[expect(unsafe_code, reason = "ffi call")]
        let handle = unsafe {
            // SAFETY:
            // - `path` is owned by this function and therefore valid for duration of this block
            // - returned handle will be stored in `Self` immediately after successful creation
            // - `Drop` calls `CloseHandle`
            CreateFile2(
                path.as_pcwstr(),
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

    /// get a currently valid file-system path, not valid across reboots but can be passed
    /// to [`std::fs::read_dir`] etc.
    pub fn fs_path(&self) -> io::Result<PathBuf> {
        let mut info: STORAGE_DEVICE_NUMBER = Default::default();
        let mut bytes_returned: u32 = 0;

        #[expect(unsafe_code, reason = "ffi call")]
        let get_info = unsafe {
            // SAFETY:
            // - handle known to be valid, owned by Self
            // - IOCTL_STORAGE_GET_DEVICE_NUMBER does not require input
            // - output buffer of type STORAGE_DEVICE_NUMBER
            // - bytes returned is not NULL
            // - not overlapped
            DeviceIoControl(
                self.0,
                const { IOCTL_STORAGE_GET_DEVICE_NUMBER.strict_cast_unsigned() },
                null(),
                0,
                &mut info as *mut _ as *mut _,
                const { size_of::<STORAGE_DEVICE_NUMBER>().strict_cast() },
                &mut bytes_returned as *mut _,
                null_mut(),
            )
        };
        (get_info != 0)
            .ok_or_else(io::Error::last_os_error)
            .or_warn("unable to get meaningful drive details")?;

        (bytes_returned == const { size_of::<STORAGE_DEVICE_NUMBER>().strict_cast::<u32>() })
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "wrong number of bytes received. Expected {correct} got {bytes_returned}",
                        correct = const { size_of::<STORAGE_DEVICE_NUMBER>() }
                    ),
                )
            })
            .or_error("")?;

        (info.DeviceType == const { FILE_DEVICE_CD_ROM.strict_cast_unsigned() })
            .ok_or_else(|| io::Error::new(ErrorKind::Unsupported, "Invalid device type"))
            .or_warn("not a CDROM")?;

        let path = PathBuf::from(format!(r"\\.\CDROM{}", info.DeviceNumber));
        Ok(path)
    }

    /// Obtain a reference to the underlying [`HANDLE`] for the drive.
    ///
    /// # SAFETY
    /// - Any modifications to the underlying [`HANDLE`] must ensure:
    ///   1. That the previous handle is properly closed
    ///   2. That the new handle is valid, open and refers to an available device which
    ///      supports [`GUID_DEVINTERFACE_CDROM`]
    /// - [`DriveHandle`] is marked as [`Send`]. Callers must ensure that the handle is not
    ///   used to enable concurrent access to the drive ("processes and threads that share
    ///   the same file must synchronize their access").
    ///   See: https://learn.microsoft.com/en-us/windows/win32/fileio/file-handles
    #[expect(
        unsafe_code,
        reason = "required to be unsafe, to allow DriveHandle to be Send"
    )]
    pub unsafe fn as_handle_mut(&mut self) -> &mut HANDLE {
        &mut self.0
    }
}

impl DriveHandle {
    /// Obtain a reference to the underlying [`HANDLE`] for the drive.
    ///
    /// # SAFETY
    /// - [`DriveHandle`] is marked as [`Send`]. Callers must ensure that the handle is not
    ///   used to enable concurrent access to the drive ("processes and threads that share
    ///   the same file must synchronize their access").
    ///   See: https://learn.microsoft.com/en-us/windows/win32/fileio/file-handles
    #[expect(
        unsafe_code,
        reason = "required to be unsafe, to allow DriveHandle to be Send"
    )]
    pub unsafe fn as_handle(&self) -> &HANDLE {
        &self.0
    }
}

#[cfg(test)]
impl TryFrom<&DriveHandle> for TestAlbum {
    type Error = io::Error;

    fn try_from(handle: &DriveHandle) -> Result<Self, Self::Error> {
        handle.0.try_into()
    }
}

pub struct InvalidHandle;

impl Try for DriveHandle {
    type Output = DriveHandle;

    type Residual = InvalidHandle;

    #[inline]
    fn from_output(output: Self::Output) -> Self {
        output
    }

    #[inline]
    fn branch(self) -> std::ops::ControlFlow<Self::Residual, Self::Output> {
        if self.0 == INVALID_HANDLE_VALUE {
            std::ops::ControlFlow::Break(InvalidHandle)
        } else {
            std::ops::ControlFlow::Continue(self)
        }
    }
}

impl FromResidual<InvalidHandle> for DriveHandle {
    #[inline]
    fn from_residual(_: InvalidHandle) -> Self {
        Self(INVALID_HANDLE_VALUE)
    }
}

impl<T> FromResidual<InvalidHandle> for io::Result<T> {
    #[inline]
    fn from_residual(_: InvalidHandle) -> Self {
        Err(io::Error::last_os_error())
    }
}

impl Residual<DriveHandle> for InvalidHandle {
    type TryType = DriveHandle;
}

trait IntoDriveHandle {
    // TODO unsafe
    fn into_drive_handle(self) -> DriveHandle;
}

impl IntoDriveHandle for HANDLE {
    #[inline]
    fn into_drive_handle(self) -> DriveHandle {
        DriveHandle(self)
    }
}

#[derive(Debug)]
pub struct DeviceInfoSet(HDEVINFO);

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
            SetupDiDestroyDeviceInfoList(self.0 as *mut _);
        }
        // Not checking for success: cannot meaningfully handle CloseHandle failure during drop
    }
}

pub trait IntoDeviceInfoSet {
    /// Stores the handle as a DeviceInfoSet with associated type-safety, drop and try implementations
    ///
    /// # SAFETY
    ///
    /// Implementation must only be performed on types which can represent handles to device
    /// information sets
    #[expect(
        unsafe_code,
        reason = "caller must guarantee correctness of wrapped handle"
    )]
    unsafe fn into_deviceinfoset(self) -> io::Result<DeviceInfoSet>;
}

impl IntoDeviceInfoSet for HDEVINFO {
    #[inline]
    #[expect(
        unsafe_code,
        reason = "caller must guarantee correctness of wrapped handle"
    )]
    /// Stores the handle as a DeviceInfoSet with associated type-safety, drop and try implementations
    ///
    /// # SAFETY
    ///
    /// Callers must ensure that the wrapped handle is either:
    /// - an open handle to a device information set
    /// - or INVALID_HANDLE_VALUE from a failed attempt to obtain a handle to a device information set
    unsafe fn into_deviceinfoset(self) -> io::Result<DeviceInfoSet> {
        if self == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(DeviceInfoSet(self))
        }
    }
}

impl DeviceInfoSet {
    pub fn for_device_class(class: Guid) -> io::Result<Self> {
        let _tracing_span =
            tracing::error_span!("DeviceInfoSet::for_device_class", %class).entered();

        #[expect(unsafe_code, reason = "ffi call")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "directly type-safe wrapped via into_deviceinfoset"
        )]
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
            .into_deviceinfoset()
        }
        .or_warn("invalid handle")?;
        Ok(handle)
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
                self.0,
                // If this parameter is NULL, repeated calls to SetupDiEnumDeviceInterfaces return
                // information about the interfaces that are associated with *all* the device
                // information elements in DeviceInfoSet
                null(),
                // A pointer to a GUID that specifies the device interface class for the
                // requested interface.
                &GUID_DEVINTERFACE_CDROM as *const _,
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
                self.0,
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
                self.0,
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
