//! Handles direct hardware access via Windows APIs

use std::{
    fmt::Debug,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use tracing::field::Empty;
use tracing_result::Trace;

use super::{
    bindings::GUID_DEVINTERFACE_CDROM,
    convert::{Guid, WinString},
    toc::{CDROM_TOC, TOC_SIZE},
};
#[cfg(test)]
use crate::test_fixtures::albums::TestAlbum;
#[cfg(any(
    target_arch = "aarch64",
    target_arch = "arm64ec",
    target_arch = "x86_64"
))]
use crate::{FRAME_SIZE, Track, hex::hex_dump};
use crate::{
    Frame,
    win::bindings::{DeviceDetails, DeviceInfoSet, DriveHandle},
};

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

    /// Read a chunk of data from the disc to `buf`. Returns number of bytes read
    ///
    /// TODO - return frames read, not bytes
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

        let start = track.toc_entry.start + frame_offset;
        let frames_read = self.handle.read_cdda(start, buf)?;
        (frames_read == Frame::new(frames_to_read.strict_cast()))
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::UnexpectedEof,
                    format!("expected {frames_to_read} frames, but only read {frames_read:?}"),
                )
            })
            .or_error("reading chunk")?;
        let bytes_read = frames_read.as_usize().strict_mul(FRAME_SIZE);
        Ok(bytes_read.strict_cast())
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
        let albums: Vec<_> = drives.map(|drive| drive.toc().to_toc().unwrap()).collect();
        let expected = [
            DefinitelyMaybe.expected_toc(),
            TheWallDisc1.expected_toc(),
            TheWallDisc2.expected_toc(),
        ];
        assert_eq!(albums, expected);
    }
}
