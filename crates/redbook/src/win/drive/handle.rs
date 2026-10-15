use crate::win::bindings::{FILE_DEVICE_CD_ROM, IOCTL_STORAGE_GET_DEVICE_NUMBER};

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
        };
        (handle != INVALID_HANDLE_VALUE)
            .ok_or_else(io::Error::last_os_error)
            .or_error("getting handle for drive")?;
        tracing::debug!(?handle, "opened successfully");
        Ok(Self(handle))
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
