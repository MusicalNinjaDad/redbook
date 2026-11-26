use std::{
    fmt::Display,
    io,
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

#[cfg(any(test, doc, not(windows)))]
use ::std::alloc::{Layout, LayoutError};

#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
use derive_dst::DST;
use tracing::{field::Empty, instrument};
use tracing_result::Trace;

use crate::{
    Device, Frame, TocEntries,
    drive::error::{DeviceError, InvalidBufferSize, OsError},
    sys::win::bindings::{
        IOCTL_STORAGE_QUERY_PROPERTY, PropertyStandardQuery, STORAGE_ADAPTER_DESCRIPTOR,
        STORAGE_PROPERTY_QUERY, StorageAdapterProperty,
    },
    toc::ByteSize,
};

use super::{
    bindings::{
        BOOL, CDDA, CDROM_READ_TOC_EX, CDROM_TOC, CloseHandle, CreateFile2, DIGCF_DEVICEINTERFACE,
        DIGCF_PRESENT, DUPLICATE_SAME_ACCESS, DeviceIoControl, DuplicateHandle,
        ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS, FALSE, FILE_DEVICE_CD_ROM, FILE_SHARE_READ,
        GENERIC_READ, GUID_DEVINTERFACE_CDROM, GetCurrentProcess, HANDLE, HDEVINFO,
        INVALID_HANDLE_VALUE, IOCTL_CDROM_RAW_READ, IOCTL_CDROM_READ_TOC_EX,
        IOCTL_STORAGE_GET_DEVICE_NUMBER, OPEN_EXISTING, RAW_READ_INFO, SP_DEVICE_INTERFACE_DATA,
        SP_DEVICE_INTERFACE_DETAIL_DATA_W, STORAGE_DEVICE_NUMBER, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    },
    convert::{Guid, Sector, WinPath, WinString},
    drive::DeviceInfoSetIter,
    error::WinError,
};

pub use safety::DriveHandle;
#[doc(inline)]
use safety::IntoDriveHandle;

#[cfg(test)]
use crate::test_fixtures::albums::TestAlbum;
/// Contains and seals the only way to create a DriveHandle, thus enforcing invariants.
mod safety {
    use crate::drive::error::OsError;

    use super::*;
    /// A open file handle which can only be created by calling
    /// [`into_drive_handle`][HANDLE::into_drive_handle], thus ensuring invariants are upheld.
    ///
    /// # SAFETY guarantees provided
    /// - Stored handle is not publicly accessible.
    /// - Handle is closed on Drop.
    /// - Can only be publically created via [open][Self::open] which validates the handle.
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
        fn into_drive_handle(self) -> Result<DriveHandle, DeviceError>;
    }

    impl IntoDriveHandle for HANDLE {
        #[inline]
        fn into_drive_handle(self) -> Result<DriveHandle, DeviceError> {
            (self != INVALID_HANDLE_VALUE)
                .ok_or_else(WinError::get_from_os)
                .or_warn("invalid handle")?;

            let drivehandle = DriveHandle(self);
            (drivehandle
                .device_number()
                .or_warn("unable to identify device type")?
                .DeviceType
                == const { FILE_DEVICE_CD_ROM.strict_cast::<u32>() })
            .ok_or(DeviceError::UnsupportedDevice)
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
///
/// # TODO
/// TODO: #118 can `DriveHandle` also be `Sync`?
#[expect(
    unsafe_code,
    reason = "Want to be able to rip in one thread and encode in another"
)]
// SAFETY: See documentation comment
unsafe impl Send for DriveHandle {}

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
    pub(super) fn device_number(&self) -> Result<STORAGE_DEVICE_NUMBER, DeviceError> {
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
    fn cdrom_toc(&self) -> Result<CDROM_TOC, DeviceError> {
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
    type OsError = WinError;
    type Iter = DeviceInfoSetIter;

    fn open<P: AsRef<Path> + ?Sized>(path: &P) -> Result<Self, DeviceError> {
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

    fn all() -> Result<DeviceInfoSetIter, DeviceError> {
        let debug = tracing::debug_span!("DriveHandle::all", deviceinfoset = Empty).entered();

        let deviceinfoset = DeviceInfoSet::for_device_class(Guid(GUID_DEVINTERFACE_CDROM))
            .or_error("getting device info set")?;

        debug.record("deviceinfoset", format!("{deviceinfoset:?}"));
        tracing::debug!("opened DeviceInfoSet");

        Ok(deviceinfoset.into_iter())
    }

    fn fs_path(&self) -> Result<PathBuf, DeviceError> {
        let info = self.device_number()?;

        let path = PathBuf::from(format!(r"\\.\CDROM{}", info.DeviceNumber));

        Ok(path)
    }

    fn read_cdda(&self, start: Frame, buf: &mut [u8]) -> Result<Frame, DeviceError> {
        let span = tracing::error_span!(
            "DriveHandle::read_cdda",
            %start,
            buf_size = buf.len(),
            frames_to_read = Empty,
            start_offset_sector = Empty,
        )
        .entered();

        let buf_size = ByteSize::new(buf.len()).or_error("invalid buffer")?;

        // SAFETY: buffer size must be >= sizeof(SectorCount * RAW_SECTOR_SIZE)
        // SectorCount = frames_to_read,  RAW_SECTOR_SIZE = ByteSize::ONE_FRAME
        //
        // Upheld by invariant that buf_size.frames() is calculated as buf.len().div(ONE_FRAME)
        // Note we are not relying on exact sizing as the unsafe ffi call does not require it.
        let frames_to_read = buf_size.frames();

        // the stored value is always <= ByteSize::MAX < u32::MAX, so casting to the DWORD (`u32`)
        // buffer size is infallible
        let bytes_to_read = buf_size.strict_cast();

        span.record("frames_to_read", format_args!("{frames_to_read}"));

        let offset = Sector::from_frame(start).offset();
        span.record("start_offset_sector", offset);

        // SAFETY: We rely on `size_of::<>()`. Changing type requires updating the
        // call to `DeviceIoControl` below
        let read_command: RAW_READ_INFO = RAW_READ_INFO {
            DiskOffset: offset,
            SectorCount: frames_to_read.cast(),
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
                // 2. For IOCTL_CDROM_RAW_READ this must be >= sizeof(SectorCount * RAW_SECTOR_SIZE)
                //    - SectorCount calculated directly from buf via buf_size.checked_exact_frames()
                bytes_to_read,
                &mut bytes_read as *mut _,
                null_mut(),
            )
        }
        .validate(Some(bytes_to_read), bytes_read)
        .or_warn("raw CDDA data read failed")?;

        Ok(frames_to_read)
    }

    fn read_toc(&self) -> Result<TocEntries, DeviceError> {
        let _debug = tracing::debug_span!("DriveHandle::read_toc").entered();
        // TODO why does this compile - check conversions are all still correct
        Ok(self.cdrom_toc()?.try_into()?)
    }

    fn raw_toc(&self) -> Result<Vec<u8>, DeviceError> {
        // TODO why does this compile - check conversions are all still correct
        Ok(self.cdrom_toc()?.as_raw_bytes())
    }

    fn raw_max_chunk(&self) -> Result<usize, DeviceError> {
        let query = STORAGE_PROPERTY_QUERY {
            PropertyId: StorageAdapterProperty,
            QueryType: PropertyStandardQuery,
            // SAFETY: Use default() to ensure all memory is initialised
            // SAFETY: No additional parameters required for this query type
            // see https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddstor/ne-ntddstor-storage_property_id
            AdditionalParameters: Default::default(),
        };

        // SAFETY: Use default() to ensure all memory is initialised
        let mut details = STORAGE_ADAPTER_DESCRIPTOR::default();
        let mut bytes_returned: u32 = 0;

        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "inline call to get handle"
        )]
        // SAFETY: inline (see mock docstring for link to spec & consolidated requirements)
        unsafe {
            DeviceIoControl(
                // handle known valid thanks to type wrapper
                *self.handle(),
                // valid constant from win_bindgen. const stict_cast ensures no overflows
                const { IOCTL_STORAGE_QUERY_PROPERTY.strict_cast() },
                // points to a valid STORAGE_PROPERTY_QUERY, constructed above, fully initialised
                // named type cast adds extra type-safety
                &query as *const STORAGE_PROPERTY_QUERY as *const _,
                // lpinbuffer is a STORAGE_PROPERTY_QUERY. const stict_cast ensures no overflows
                const { size_of::<STORAGE_PROPERTY_QUERY>().strict_cast() },
                // For query type PropertyStandardQuery / StorageAdapterProperty -> STORAGE_ADAPTER_DESCRIPTOR,
                // constructed above, fully initialised named type cast adds extra type-safety
                &mut details as *mut STORAGE_ADAPTER_DESCRIPTOR as *mut _,
                // QueryType: PropertyStandardQuery -> must be > 0 and correct size for output.
                // const stict_cast ensures no overflows
                const { size_of::<STORAGE_ADAPTER_DESCRIPTOR>().strict_cast() },
                &mut bytes_returned as *mut _,
                null_mut(),
            )
        }
        .validate(
            Some(const { size_of::<STORAGE_ADAPTER_DESCRIPTOR>().strict_cast() }),
            bytes_returned,
        )
        .or_error("identifying maximum readable chunk size failed")?;

        Ok(details.MaximumTransferLength.strict_cast())
    }
}

trait IoControl {
    fn validate(&self, bytes_expected: Option<u32>, bytes_returned: u32)
    -> Result<(), DeviceError>;
}

/// TODO #244 Document and fix tracing semantics of IoControl::validate whcih is now also used for other ffi calls
impl IoControl for BOOL {
    fn validate(
        &self,
        bytes_expected: Option<u32>,
        bytes_returned: u32,
    ) -> Result<(), DeviceError> {
        let _span = tracing::error_span!("DeviceIoControl::validate", return_code = self, ?bytes_expected, %bytes_returned).entered();

        (*self != 0)
            .ok_or_else(WinError::get_from_os)
            .or_warn("error calling DeviceIoControl")?;

        if let Some(bytes_expected) = bytes_expected {
            (bytes_returned == bytes_expected)
                .ok_or(DeviceError::UnexpectedEndOfRead {
                    expected: bytes_expected,
                    received: bytes_returned,
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
/// Wrapper around [`HDEVINFO`] (provided by [`SetupDiGetClassDevsW`]) with `Drop` implementation
/// to close handle & additional knowledge of the requersted class [`Guid`].
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
    pub fn for_device_class(class: Guid) -> Result<Self, WinError> {
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
            Err(WinError::from(io::Error::last_os_error())).or_warn("invalid handle")?
        } else {
            Ok(Self { handle, class })
        }
    }

    /// Obtain the GUID of the associated device class
    pub fn class(&self) -> Guid {
        self.class
    }

    /// Get the detail data for a given member of the device information set
    ///
    /// Returns Ok(None) if the member does not exist
    pub fn get_member(&self, index: usize) -> Result<Option<Box<DeviceDetails>>, DeviceError> {
        // This is an wonderful, 3-step process in the C++ API so we expose it as a single pub fn
        // and wrap each step in a private function to try and preserve our, and your, sanity.

        let interface_data = match self.get_interface_data(index.strict_cast())? {
            Some(data) => data,
            None => return Ok(None),
        };

        let member = self.get_member_details(&interface_data)?;

        Ok(Some(member))
    }

    /// Provides the basic information required to be able to get detail data for a given member.
    ///
    /// Returns `Ok(None)` if the member does not exist.
    fn get_interface_data(
        &self,
        memberindex: u32,
    ) -> Result<Option<SP_DEVICE_INTERFACE_DATA>, WinError> {
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
                Err(WinError::from(err))
            }
            _ => {
                tracing::debug!(guid = %Guid(deviceinterfacedata.InterfaceClassGuid), "got data for device");
                Ok(Some(deviceinterfacedata))
            }
        }
    }

    /// Identifies the required size for a DeviceDetails buffer, allocates and returns it.
    fn allocate_device_details(
        &self,
        member: &SP_DEVICE_INTERFACE_DATA,
    ) -> Result<Box<DeviceDetails>, DeviceError> {
        let _span = tracing::error_span!("DeviceInfoSet::allocate_device_details").entered();
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
        let err = WinError::get_from_os();
        (get_required_buffer_size == 0 && err.error_code() == ERROR_INSUFFICIENT_BUFFER)
            .ok_or(err)
            .or_error("reading required buffer size")?;

        tracing::debug!(requiredsize);

        Ok(DeviceDetails::new(requiredsize.strict_cast())?)
    }

    /// Get detail data for a given `member`.
    fn get_member_details(
        &self,
        member: &SP_DEVICE_INTERFACE_DATA,
    ) -> Result<Box<DeviceDetails>, DeviceError> {
        let _span = tracing::error_span!("DeviceInfoSet::get_member_details").entered();

        // SAFETY:
        // Caller must guarantee that the buffer struct is correctly sized and initialised
        // before calling SetupDiGetDeviceInterfaceDetailW
        let deviceinterfacedetaildata = self.allocate_device_details(member)?;
        let size = size_of_val(&*deviceinterfacedetaildata);

        // SAFETY:
        // We will rebox the value after calling devicedetailsptr, whether or not an error occurs.
        // This ensures that the value is correctly dropped.
        let devicedetailsptr = Box::into_raw(deviceinterfacedetaildata);

        // SAFETY: https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceinterfacedetailw
        unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                // Guaranteed valid, non-null handle during construction of Self
                self.handle,
                // Reference to a &SP_DEVICE_INTERFACE_DATA must point to a valid value of that type
                member,
                // allocate_device_details (above) calls SetupDiGetDeviceInterfaceDetailW to request
                // the required size of this buffer struct. It then allocated and returns a valid,
                // correctly sized Box<DeviceDetails>.
                //
                // The Box which contained this has just been converted and is under our management
                // until we rebox it after this call
                //
                // *mut DeviceDetails can be safely cast to *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W
                // as it shares an identical layout.
                devicedetailsptr as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W,
                // This is calculated as the size of the backing buffer above - therefore no read
                // out of bounds risk.
                size.strict_cast(),
                null_mut(),
                null_mut(),
            )
        }
        .validate(None, 0)
        .inspect_err(|_| {
            // SAFETY:
            // This is the same pointer which we previously unboxed before passing to
            // SetupDiGetDeviceInterfaceDetailW
            // reboxing here so that it is properly dropped on error
            let details = unsafe { Box::from_raw(devicedetailsptr) };
            drop(details);
        })
        .or_error("getting device details")?;

        // SAFETY:
        // This is the same pointer which we previously unboxed before passing to
        // SetupDiGetDeviceInterfaceDetailW. It was valid when we took it out of the box
        // and is therefore still valid now.
        let details = unsafe { Box::from_raw(devicedetailsptr) };
        Ok(details)
    }
}

// TODO: #243 support x86 (32-bit)??
// #[repr(C, packed(1))]
// #[cfg(target_arch = "x86")]
// #[expect(nonstandard_style, reason = "mimic C++ struct")]
// /// A custom variant of [SP_DEVICE_INTERFACE_DETAIL_DATA_W] with a pre-allocated buffer
// /// large enough for any valid drive path (win32 MAX_PATH = 260 char)
// #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
// pub(super) struct DeviceDetails {
//     cbSize: u32 = const {size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32},
//     DevicePath: [u16; MAX_PATH_CHARS] = [0; _],
// }

#[repr(C)]
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
#[expect(non_snake_case, reason = "mimic C++ struct")]
/// A layout-identical version of [SP_DEVICE_INTERFACE_DETAIL_DATA_W] as a DST.
///
/// As this is not `Sized` it can only be held in a `Box` etc. [`new`][Self::new]
/// allocates memory for a for a given sized `DeviceDetails` and returns it boxed and ready to go.
#[derive(Debug, PartialEq, Eq, DST)]
pub(crate) struct DeviceDetails {
    cbSize: u32,
    DevicePath: [u16],
}

impl DeviceDetails {
    #[instrument(level = "error", fields(layout, offset, num_words))]
    /// Construct a new [`DeviceDetails`] of the given size and return it in a `Box`.
    ///
    /// # Note
    /// - The field cbSize will be correctly set to the size of a DeviceDetails *with a single
    ///   `0_u16` terminator in the DevicePath*. The [docs][MsLearn] claim "The cbSize member always
    ///   contains the size of the fixed part of the data structure, not a size reflecting the
    ///   variable-length string at the end." However trial and (lots of) error have shown that
    ///   the buffer will be refused with OsError 1784 `ERROR_INVALID_USER_BUFFER` for any other
    ///   value of cbSize.
    /// - This will error if it is not possible to layout a valid DeviceDetails at the requested
    ///   size. [`SetupDiGetDeviceInterfaceDetailW`] provides the full padded size, e.g. for a
    ///   path length of 122 chars a size of 252 is requested to cover cbSize, null-terminator &
    ///   padding.
    ///
    /// [MsLearn]: https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceinterfacedetailw#parameters
    pub fn new(size: usize) -> Result<Box<Self>, InvalidBufferSize> {
        tracing::debug!("new Device Details");

        let (mut raw, (layout, offset, num_words)) = Self::new_uninit_for_size(size)?;
        tracing::Span::current().record("layout", format_args!("{layout:?}"));
        tracing::Span::current().record("offset", offset);
        tracing::Span::current().record("num_words", num_words);
        tracing::debug!("allocated");

        raw.cbSize
            .write(const { size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32 });

        // SAFETY:
        // - all fields (u32, u16) are valid when zeroed
        Ok(unsafe { raw.assume_init() })
    }

    #[cfg(any(test, doc, not(windows)))]
    pub fn layout(size: usize) -> Result<(Layout, usize, usize), LayoutError> {
        Self::layout_for_size(size)
    }

    /// This path is valid across reboots and valid to pass directly to [`CreateFile2`]
    pub fn path(&self) -> PathBuf {
        let path = WinString::from(&self.DevicePath);
        PathBuf::from(path.to_string())
    }
}

impl Display for DeviceDetails {
    /// Output the path, parsing correctly as null-terminated utf16
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Display::fmt(&self.path().display(), f)
    }
}

#[cfg(test)]
mod miri {
    use std::{alloc::Layout, mem::offset_of};

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

    #[test]
    fn device_details_layout() {
        let original = SP_DEVICE_INTERFACE_DETAIL_DATA_W::default();
        let size = size_of_val(&original);

        let custom = DeviceDetails::new(size).unwrap();

        assert_eq!(Layout::for_value(&original), Layout::for_value(&*custom));
        assert_eq!(
            offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, cbSize),
            offset_of!(DeviceDetails, cbSize)
        );
        assert_eq!(
            offset_of!(SP_DEVICE_INTERFACE_DETAIL_DATA_W, DevicePath),
            offset_of!(DeviceDetails, DevicePath)
        );
    }
}
