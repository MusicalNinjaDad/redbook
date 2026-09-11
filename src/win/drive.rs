//! Handles direct hardware access via Windows APIs
use std::{
    fmt::{Debug, Display},
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

use tracing::field::Empty;
use tracing_result::Trace;

use super::{
    MAX_PATH_CHARS,
    bindings::{
        CDDA, CDROM_TOC, CloseHandle, CreateFile2, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT,
        DeviceIoControl, FILE_SHARE_READ, GENERIC_READ, GUID_DEVINTERFACE_CDROM, HANDLE, HDEVINFO,
        INVALID_HANDLE_VALUE, IOCTL_CDROM_RAW_READ, OPEN_EXISTING, RAW_READ_INFO,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W, SetupDiEnumDeviceInterfaces,
        SetupDiGetClassDevsW, SetupDiGetDeviceInterfaceDetailW,
    },
    convert::{Guid, Sector, WinPath, WinString},
    toc::TOC_SIZE,
};
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
use crate::{
    FRAME_SIZE, Track,
    hex::hex_dump,
    win::bindings::{ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS},
};

pub(super) use handle::DriveHandle;

/// A CdDrive with opened read-only [`HANDLE`] and [`CDROM_TOC`]
///
/// # SAFETY
/// - CdDrive cannot be `Clone` to avoid duplicate handles
pub struct CdDrive {
    path: WinPath,
    handle: DriveHandle,
    toc: CDROM_TOC,
}

/// # SAFETY
/// - The only way to get the underlying [`HANDLE`] is via `unsafe` call to [`handle`][Self::handle]
///   which includes specific safety restrictions allowing `CdDrive` to be [Send]
/// - All other fields are already `Send`
/// - Not Sync as we have not enabled overlapped I/O or any internal sync mechanism
#[expect(
    unsafe_code,
    reason = "Want to be able to rip in one thread and encode in another"
)]
// SAFETY: See documentation comment
unsafe impl Send for CdDrive {}

impl Debug for CdDrive {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CdDrive")
            .field("path", &self.path)
            .field("handle", &self.handle)
            .field("toc", &self.toc_as_raw_bytes())
            .finish()
    }
}

impl PartialEq for CdDrive {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
            && self.handle == other.handle
            && self.toc_as_raw_bytes() == other.toc_as_raw_bytes()
    }
}

impl Eq for CdDrive {}

impl CdDrive {
    /// A safe wrapper around the ffi calls needed to obtain a handle for the raw
    /// drive at `path` and obtain the TOC as provided by the relevant windows system
    /// call `DeviceIoControl(..,IOCTL_CDROM_READ_TOC_EX,..)`
    ///
    /// - The returned [`CdDrive`] provides methods to access the handle and TOC.
    /// - The handle has minimal (shared read only) access rights and will be closed
    ///   when the [`CdDrive`] is dropped. Consider using [exit_safely] to ensure that
    ///   this occurs in your binary even on error.
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path: PathBuf = PathBuf::from(path.as_ref());
        let path_str = path.display().to_string();

        let _error = tracing::error_span!("CdDrive::open", path = %path_str).entered();

        let windrive = WinString::from(format!(r"\\.\{}", path.display()));
        let mut handle = DriveHandle::open(windrive).or_error("")?;
        let toc = CDROM_TOC::read_from(&mut handle)?;
        Ok(Self {
            path: path.into(),
            handle,
            toc,
        })
    }

    /// The path of the drive
    pub fn path(&self) -> &WinPath {
        &self.path
    }

    /// Obtain a reference to the [`HANDLE`] for the drive.
    ///
    /// # SAFETY
    /// - [`CdDrive`] is marked as [`Send`]. Callers must ensure that the handle is not
    ///   used to enable concurrent access to the drive ("processes and threads that share
    ///   the same file must synchronize their access").
    ///   See: https://learn.microsoft.com/en-us/windows/win32/fileio/file-handles
    #[expect(
        unsafe_code,
        reason = "required to be unsafe, to allow CdDrive to be Send"
    )]
    pub unsafe fn handle(&self) -> &HANDLE {
        // SAFETY: We provide the same safety message regarding `Send`
        unsafe { self.handle.as_handle() }
    }

    /// Obtain an array of raw bytes representing the [`CDROM_TOC`]
    pub fn toc_as_raw_bytes(&self) -> &[u8] {
        #[expect(unsafe_code, reason = "need to construct slice from raw parts")]
        unsafe {
            // SAFETY: check stored value is the expected size
            assert_eq!(size_of_val(&self.toc), TOC_SIZE);
            std::slice::from_raw_parts(&self.toc as *const _ as *const _, TOC_SIZE)
        }
    }

    /// Obtain a hex representation of the raw bytes representing the [`CDROM_TOC`]
    pub fn toc_as_hex(&self) -> String {
        hex_dump(self.toc_as_raw_bytes())
    }

    /// Obtain a reference to the TOC as a [`CDROM_TOC`]
    pub fn toc(&self) -> &CDROM_TOC {
        &self.toc
    }

    /// Read a chunk of data from the disc to `buf`
    pub fn read_chunk(
        &self,
        track: &Track,
        frame_offset: usize,
        frames_to_read: u32,
        buf: &mut [u8],
    ) -> io::Result<u32> {
        let _trace = tracing::trace_span!(
            "CdDrive::read_chunk",
            track = track.toc_entry.track,
            frame_offset,
            frames_to_read
        )
        .entered();
        let offset = Sector::from_frame(track.toc_entry.start + frame_offset).offset();
        let read_command = RAW_READ_INFO {
            DiskOffset: offset,
            SectorCount: frames_to_read,
            TrackMode: CDDA,
        };

        let bytes_to_read = frames_to_read * FRAME_SIZE as u32;

        let mut bytes_read: u32 = 0;
        tracing::trace!(offset = offset);

        #[expect(unsafe_code, reason = "ffi call")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "embedded call to ensure raw pointer dropped immediately"
        )]
        // SAFETY: inline based on https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_raw_read
        let read_chunk = unsafe {
            // SAFETY check: Buffer is expected size.
            // Runtime check as `buf` is provided by caller
            (bytes_to_read == buf.len() as u32)
                .ok_or_else(|| io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("buffer incorrectly sized for track data. Require {bytes_to_read} bytes, buffer is {len} bytes", len = buf.len())
                )
            )?;

            // SAFETY check: Buffer is exact size for Sector count.
            // Debug check as we generated SectorCount and have validated bytes_to_read above.
            debug_assert_eq!(
                read_command.SectorCount,
                bytes_to_read
                    .div_exact(FRAME_SIZE.try_into().unwrap())
                    .expect("no remainder")
            );

            DeviceIoControl(
                *self.handle(),
                const { IOCTL_CDROM_RAW_READ.strict_cast_unsigned() },
                // If the IOCTL is from user mode, Irp->AssociatedIrp.SystemBuffer contains a RAW_READ_INFO
                // structure that specifies the starting disk offset, the sector count, and the track mode
                // (XA or CDDA) for the read.
                &read_command as *const _ as *const _,
                // Parameters.DeviceIoControl.InputBufferLength specifies the size, in bytes, of the
                // structure, which must be >= sizeof(RAW_READ_INFO)
                size_of_val(&read_command) as u32,
                // Cannot reallocate without risking invalidating pointer. We create frame with capacity
                // equal to read_command.SectorCount * Sectorsize.
                buf as *mut _ as *mut _,
                // Parameters.DeviceIoControl.OutputBufferLength
                // specifies the size of the buffer to be read, which must be >= sizeof(SectorCount * RAW_SECTOR_SIZE)
                bytes_to_read,
                &mut bytes_read as *mut _,
                null_mut(),
            )
        };
        let _warn = tracing::warn_span!(
            "Read chunk",
            track = track.track_number(),
            frame_offset,
            offset,
            frames_to_read,
            bytes_to_read,
            bytes_read
        )
        .entered();
        if read_chunk == 0 {
            let error = io::Error::last_os_error();
            tracing::warn!(error = error.to_string());
            return Err(error);
        }
        if bytes_read != bytes_to_read {
            tracing::warn!("incorrect number of bytes read");
            return Err(io::Error::new(
                ErrorKind::Interrupted,
                format!(
                    "intended to read {bytes_to_read} bytes from offset {offset} but only got {bytes_read}"
                ),
            ));
        }
        Ok(bytes_read)
    }
}

impl TryFrom<DeviceDetails> for CdDrive {
    type Error = io::Error;

    fn try_from(device: DeviceDetails) -> Result<Self, Self::Error> {
        let mut handle = DriveHandle::open(device.path()).or_error("")?;
        let toc = CDROM_TOC::read_from(&mut handle)?;
        let path = WinPath::DevicePath(device.path());
        Ok(Self { path, handle, toc })
    }
}

/// Get all the available drives, which have an AudioCd present
pub fn all_drives() -> io::Result<CdDrives> {
    let debug = tracing::debug_span!("all_drives", handle = Empty).entered();

    #[expect(unsafe_code, reason = "ffi call")]
    // SAFETY: inline based on:
    // https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetclassdevsw
    let deviceinfoset = unsafe {
        SetupDiGetClassDevsW(
            // A pointer to the GUID for a device setup class or a device interface class.
            &GUID_DEVINTERFACE_CDROM as *const _,
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
    debug.record("handle", format!("{deviceinfoset:?}"));

    // If the operation succeeds, SetupDiGetClassDevs returns a handle to a device information
    // set that contains all installed devices that matched the supplied parameters. If the
    // operation fails, the function returns INVALID_HANDLE_VALUE. To get extended error
    // information, call GetLastError.
    (deviceinfoset != INVALID_HANDLE_VALUE)
        .ok_or_else(io::Error::last_os_error)
        .or_warn("invalid handle")?;

    tracing::debug!("got device infoset");
    Ok(CdDrives { deviceinfoset, .. })
}

/// Iterator over all the available drives, which have an AudioCd present
pub struct CdDrives {
    /// # SAFETY:
    /// Must be a valid handle (pointer *mut c_void) to device information set.
    /// Can only be constructed via [`all_drives`] which ensures this is upheld.
    deviceinfoset: HDEVINFO,
    /// Index of next element to retrieve from deviceinfoset.
    /// `u32` as this is what the ffi calls use.
    current_index: u32 = 0,
}

impl Iterator for CdDrives {
    type Item = CdDrive;

    fn next(&mut self) -> Option<Self::Item> {
        let drive_index = self.current_index;
        let deviceinfoset = self.deviceinfoset;
        self.current_index += 1;
        let debug = tracing::debug_span!(
            "next drive",
            drive_index,
            guid = Empty,
            path_length = Empty,
            path = Empty,
        )
        .entered();

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
                // SAFETY: Known to be a valid handle as this comes from non-public field in Self
                deviceinfoset,
                // If this parameter is NULL, repeated calls to SetupDiEnumDeviceInterfaces return
                // information about the interfaces that are associated with *all* the device
                // information elements in DeviceInfoSet
                null(),
                // A pointer to a GUID that specifies the device interface class for the
                // requested interface.
                &GUID_DEVINTERFACE_CDROM as *const _,
                // A zero-based index into the list of interfaces in the device information set.
                drive_index,
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

        let err = io::Error::last_os_error();
        // repeatedly increment MemberIndex and retrieve an interface until this function
        // fails and GetLastError returns ERROR_NO_MORE_ITEMS
        match (get_data, err.raw_os_error()) {
            (0, Some(ERROR_NO_MORE_ITEMS)) => return None,
            (0, _) => {
                tracing::error!(drive_index, %err, "failed to get data about device");
                return None;
            }
            _ => debug.record(
                "guid",
                Guid(deviceinterfacedata.InterfaceClassGuid).to_string(),
            ),
        };

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
                deviceinfoset,
                &deviceinterfacedata as *const _,
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
            (0, Some(ERROR_INSUFFICIENT_BUFFER)) => debug.record("path_length", requiredsize),
            _ => {
                tracing::error!(%err, "reading required buffer size");
                return None;
            }
        };

        // SAFETY:
        // Buffer required to be large enough
        try bikeshed io::Result<()> { DeviceDetails::check_size(requiredsize).or_error("")? }
            .ok()?;

        // SAFETY:
        // 1. cbSize is fixed to correct value via construction:
        //    the caller must set DeviceInterfaceDetailData.cbSize to
        //    sizeof(SP_DEVICE_INTERFACE_DETAIL_DATA) before calling SetupDiGetDeviceInterfaceDetailW.
        //    The cbSize member always contains the size of the fixed part of the data structure,
        //    not a size reflecting the variable-length string at the end.
        //
        // 2. buffer length is validated as large enough via call to DeviceDetails::check_size above
        let mut deviceinterfacedetaildata = DeviceDetails::default();

        #[expect(unsafe_code, reason = "ffi call")]
        // SAFETY: https://learn.microsoft.com/en-us/windows/win32/api/setupapi/nf-setupapi-setupdigetdeviceinterfacedetailw
        let get_details = unsafe {
            SetupDiGetDeviceInterfaceDetailW(
                // A pointer to a device information set that contains the device
                // interfaces for which to return information.
                // UNSAFE DO NOT KEEP THIS PUBlIC
                deviceinfoset,
                // A pointer to an SP_DEVICE_INTERFACE_DATA structure that specifies the interface
                // in DeviceInfoSet for which to retrieve details. A pointer of this type is
                // typically returned by SetupDiEnumDeviceInterfaces.
                &deviceinterfacedata as *const _,
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
                // 2. buffer size validated via call to get requiredsize above
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
                tracing::error!(get_details, err = %io::Error::last_os_error());
                return None;
            }
            _ => debug.record("path", deviceinterfacedetaildata.to_string()),
        };

        match CdDrive::try_from(deviceinterfacedetaildata) {
            Ok(cddrive) => Some(cddrive),
            Err(error) if error.raw_os_error() == Some(21) => {
                // OS error 21 (device not ready) = no disc in drive
                drop(debug);
                self.next()
            }
            Err(error) => {
                tracing::error!(%error, "opening device");
                None
            }
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
/// `\\\\?\\usbstor#cdrom&ven_hl-dt-st&prod_dvdram_gue1n&rev_as00#4b4d444642414d3130353920&0#{53f56308-b6bf-11d0-94f2-00a0c91efb8b}\0`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DeviceDetails {
    cbSize: u32 = const {size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32},
    DevicePath: [u16; MAX_PATH_CHARS] = [0; _],
}

#[repr(C, packed(1))]
#[cfg(target_arch = "x86")]
#[expect(nonstandard_style, reason = "mimic C++ struct")]
/// A custom variant of [SP_DEVICE_INTERFACE_DETAIL_DATA_W] with a pre-allocated buffer
/// large enough for any valid drive path (win32 MAX_PATH = 260 char)
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DeviceDetails {
    cbSize: u32 = const {size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32},
    DevicePath: [u16; MAX_PATH_CHARS] = [0; _],
}

impl DeviceDetails {
    /// Validate that the buffer provided by `DeviceDetails` is sufficient.
    ///
    /// It is recommended to first call `SetupDiGetDeviceInterfaceDetailW` as per C++ docs to
    /// get the required size, then to call `check_size` before using DeviceDetails to store the
    /// information provided by a second call to `SetupDiGetDeviceInterfaceDetailW`
    fn check_size(requiredsize: u32) -> io::Result<()> {
        (requiredsize <= size_of::<Self>() as u32)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "device path too long"))
    }

    /// This path is valid across reboots and valid to pass directly to [`CreateFile2`]
    pub fn path(&self) -> WinString {
        WinString::from(self.DevicePath.as_slice())
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

mod handle {

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
            let handle: HANDLE = unsafe {
                // SAFETY:
                // - All parameter values constructed with provided consts, no magic numbers used
                // - path is owned by this function and therefore valid for duration of this block
                // - See https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2

                let dwdesiredaccess = GENERIC_READ;
                let dwsharemode = const { FILE_SHARE_READ.strict_cast_unsigned() };
                let dwcreationdisposition = const { OPEN_EXISTING.strict_cast_unsigned() };

                CreateFile2(
                    path.as_pcwstr(),
                    dwdesiredaccess,
                    dwsharemode,
                    dwcreationdisposition,
                    null(),
                )
            };
            // If the function fails, the return value is INVALID_HANDLE_VALUE.
            // To get extended error information, call GetLastError.
            if handle == INVALID_HANDLE_VALUE {
                let error = io::Error::last_os_error();
                tracing::error!(name: "getting handle for drive", %error);
                return Err(error);
            };
            tracing::debug!(?handle, "opened successfully");
            Ok(Self(handle))
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
}

#[cfg(test)]
mod miri {
    use super::*;

    #[test]
    #[should_panic(expected = "dm")]
    fn all() {
        let mut drives = all_drives().unwrap();
        let _dm = drives.next().unwrap();
    }
}
