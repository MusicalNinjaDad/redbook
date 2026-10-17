//! Handles direct hardware access via Windows APIs

use std::{
    fmt::{Debug, Display},
    io::{self, ErrorKind},
    mem::offset_of,
    path::{Path, PathBuf},
    ptr::null_mut,
};

use tracing::field::Empty;
use tracing_result::Trace;

use super::{
    MAX_PATH_CHARS,
    bindings::{
        CDDA, DeviceIoControl, GUID_DEVINTERFACE_CDROM, HANDLE, IOCTL_CDROM_RAW_READ,
        RAW_READ_INFO, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    },
    convert::{Guid, Sector, WinString},
    toc::{CDROM_TOC, TOC_SIZE},
};
#[cfg(test)]
use crate::test_fixtures::albums::TestAlbum;
use crate::win::handles::{DeviceInfoSet, DriveHandle};
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
use crate::{FRAME_SIZE, Track, hex::hex_dump};

#[expect(
    rustdoc::private_intra_doc_links,
    reason = "1. in-IDE help, 2. lint enforcement of privacy"
)]
/// A CdDrive with opened read-only [`HANDLE`] and [`CDROM_TOC`]
///
/// # SAFETY
/// - CdDrive cannot be `Clone` to avoid duplicate handles
pub struct CdDrive {
    path: PathBuf,
    handle: DriveHandle,
    toc: CDROM_TOC,
}

#[expect(
    rustdoc::private_intra_doc_links,
    reason = "1. in-IDE help, 2. lint enforcement of privacy"
)]
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
            .field("toc", &self.toc_as_hex())
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
        let handle = DriveHandle::open(windrive).or_error("opening drive")?;
        let toc = handle.read_toc().or_error("reading TOC")?;
        Ok(Self { path, handle, toc })
    }

    /// The path of the drive
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    #[expect(
        rustdoc::private_intra_doc_links,
        reason = "1. in-IDE help, 2. lint enforcement of privacy"
    )]
    /// Obtain a reference to the [`HANDLE`] for the drive.
    ///
    /// # SAFETY
    /// - Callers must not call CloseHandle, to avoid double-close when DriveHandle is dropped
    /// - [`CdDrive`] is marked as [`Send`]. Callers must ensure that the handle is not
    ///   used to enable concurrent access to the drive ("processes and threads that share
    ///   the same file must synchronize their access").
    ///   See: [MS Learn](https://learn.microsoft.com/en-us/windows/win32/fileio/file-handles)
    #[expect(
        unsafe_code,
        reason = "required to be unsafe, to allow CdDrive to be Send"
    )]
    pub unsafe fn handle(&self) -> &HANDLE {
        // SAFETY: We provide the same safety message regarding  CloseHandle & `Send`
        unsafe { self.handle.as_handle() }
    }

    /// Obtain the raw bytes representing the [`CDROM_TOC`]
    pub fn toc_as_raw_bytes(&self) -> Vec<u8> {
        let toc: CDROM_TOC = self.toc;

        // SAFETY check: stored value is the expected size, nothing has changed in the ffi spec
        const { assert!(size_of::<CDROM_TOC>() == TOC_SIZE) };

        // SAFETY: toc.Length Indicates the length, in bytes, of the table of contents data.
        // This length value does not include the length of the Length member itself.
        let len = u16::from_be_bytes(toc.Length)
            .strict_cast::<usize>()
            .strict_add(size_of_val(&toc.Length));
        assert!(len <= TOC_SIZE);

        #[expect(unsafe_code, reason = "need to construct slice from raw parts")]
        let bytes: &[u8] = unsafe {
            // SAFETY:
            // - correct size validated above, we are not reading past the end of valid data
            // - CDROM_TOC is Copy, slice points to current stack. Only valid for duration of
            //   current function. Return type is owned copy of this data as a Vec<u8>.
            std::slice::from_raw_parts(&toc as *const _ as *const _, len)
        };

        Vec::from(bytes)
    }

    /// Obtain a hex representation of the raw bytes representing the [`CDROM_TOC`]
    pub fn toc_as_hex(&self) -> String {
        hex_dump(&self.toc_as_raw_bytes())
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

        // SAFETY: We rely on `size_of::<>()`. Changing type requires updating the
        // call to `DeviceIoControl` below
        let read_command: RAW_READ_INFO = RAW_READ_INFO {
            DiskOffset: offset,
            SectorCount: frames_to_read,
            TrackMode: CDDA,
        };

        let bytes_to_read = frames_to_read
            .checked_mul(const { FRAME_SIZE.strict_cast::<u32>() })
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::OutOfMemory,
                    "requested too many bytes for architecture",
                )
            })?;
        // SAFETY check:
        // Buffer is expected size. This is a runtime check because `buf` is provided by caller.
        (bytes_to_read == u32::try_from(buf.len()).map_err(|_| io::Error::new(ErrorKind::OutOfMemory, "buffer too large for architecture"))?)
                .ok_or_else(|| io::Error::new(
                    ErrorKind::InvalidInput,
                    format!("buffer incorrectly sized for track data. Require {bytes_to_read} bytes, buffer is {len} bytes", len = buf.len())
                )
            )?;

        let mut bytes_read: u32 = 0;
        tracing::trace!(offset = offset);

        #[expect(unsafe_code, reason = "ffi call")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "embedded call to ensure raw pointer dropped immediately"
        )]
        // SAFETY: inline based on https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_raw_read
        let read_chunk = unsafe {
            DeviceIoControl(
                *self.handle(),
                const { IOCTL_CDROM_RAW_READ.strict_cast_unsigned() },
                // If the IOCTL is from user mode, Irp->AssociatedIrp.SystemBuffer contains a RAW_READ_INFO
                // structure that specifies the starting disk offset, the sector count, and the track mode
                // (XA or CDDA) for the read.
                &read_command as *const _ as *const _,
                // Parameters.DeviceIoControl.InputBufferLength specifies the size, in bytes, of the
                // structure, which must be >= sizeof(RAW_READ_INFO)
                const { size_of::<RAW_READ_INFO>().strict_cast() },
                // Cannot reallocate without risking invalidating pointer. We create frame with capacity
                // equal to read_command.SectorCount * Sectorsize.
                buf as *mut _ as *mut _,
                // SAFETY:
                // 1. bytes_to_read == buf.len() is validated at runtime, above
                // 2. This must be >= sizeof(SectorCount * RAW_SECTOR_SIZE)
                //    - SectorCount = frames_to_read
                //    - RAW_SECTOR_SIZE = FRAME_SIZE
                //    - bytes_to_read = (frames_to_read * FRAME_SIZE) == (SectorCount * RAW_SECTOR_SIZE)
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
        let handle = DriveHandle::open(device.path()).or_error("")?;
        let toc = handle.read_toc()?;
        let path = handle.fs_path()?;
        Ok(Self { path, handle, toc })
    }
}

/// Iterator over all the available drives, which have an AudioCd present
pub struct CdDrives {
    /// # SAFETY:
    /// Must be a valid handle (pointer *mut c_void) to device information set.
    /// Can only be constructed via [`all_drives`] which ensures this is upheld.
    deviceinfoset: DeviceInfoSet,
    /// Index of next element to retrieve from deviceinfoset.
    /// `u32` as this is what the ffi calls use.
    current_index: u32 = 0,
}

impl Iterator for CdDrives {
    type Item = CdDrive;

    fn next(&mut self) -> Option<Self::Item> {
        let drive_index = self.current_index;
        self.current_index += 1;

        let deviceinfoset = &self.deviceinfoset;

        let debug = tracing::debug_span!(
            "next drive",
            drive_index,
            guid = Empty,
            path_length = Empty,
            path = Empty,
        )
        .entered();

        let drive_data = deviceinfoset
            .get_member(drive_index.strict_cast())
            .ok()
            .flatten()
            .or_warn("getting drive data")?;

        match CdDrive::try_from(drive_data) {
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
/// `\\?\usbstor#cdrom&ven_hl-dt-st&prod_dvdram_gue1n&rev_as00#4b4d444642414d3130353920&0#{53f56308-b6bf-11d0-94f2-00a0c91efb8b}\0`
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DeviceDetails {
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
    pub fn path(&self) -> WinString {
        WinString::from(self.DevicePath.as_slice())
    }

    /// Set the path. Returns the number of u16 "words" stored on success, or
    /// `ErrorKind::InvalidData` if the provided path is too large.
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

/// Get all the available drives, which have an AudioCd present
pub fn all_drives() -> io::Result<CdDrives> {
    let debug = tracing::debug_span!("all_drives", handle = Empty).entered();

    let deviceinfoset = DeviceInfoSet::for_device_class(Guid(GUID_DEVINTERFACE_CDROM))?;
    debug.record("handle", format!("{deviceinfoset:?}"));

    tracing::debug!("got device infoset");
    Ok(CdDrives { deviceinfoset, .. })
}

#[cfg(test)]
impl TryFrom<&CdDrive> for TestAlbum {
    type Error = io::Error;

    fn try_from(drive: &CdDrive) -> Result<Self, Self::Error> {
        let handle = &drive.handle;
        handle.try_into()
    }
}

#[cfg(test)]
mod miri {
    use crate::test_fixtures::albums::TestAlbum::*;

    use super::*;

    #[test]
    fn all() {
        let drives = all_drives().unwrap();
        let albums: Vec<_> = drives.map(|drive| drive.toc().as_toc().unwrap()).collect();
        let expected = [
            DefinitelyMaybe.expected_toc(),
            TheWallDisc1.expected_toc(),
            TheWallDisc2.expected_toc(),
        ];
        assert_eq!(albums, expected);
    }
}
