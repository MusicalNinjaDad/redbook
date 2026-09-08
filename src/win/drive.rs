//! Handles direct hardware access via Windows APIs

use std::{
    fmt::Debug,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    ptr::{null, null_mut},
};

use super::bindings::{
    CDROM_READ_TOC_EX, CDROM_TOC, CloseHandle, CreateFile2, DeviceIoControl, FILE_SHARE_READ,
    GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, IOCTL_CDROM_RAW_READ, IOCTL_CDROM_READ_TOC_EX,
    OPEN_EXISTING, PCWSTR, RAW_READ_INFO, TRACK_MODE_TYPE,
};

use super::toc::TOC_SIZE;
use crate::hex::hex_dump;
use crate::{FRAME_SIZE, Frame, Track};

//(?) https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ne-ntddcdrm-_track_mode_type
pub const TRACK_MODE_CDDA: TRACK_MODE_TYPE = 2;

/// A CdDrive with opened read-only [`HANDLE`] and [`CDROM_TOC`]
///
/// # SAFETY
/// - CdDrive cannot be `Clone` to avoid duplicate handles
#[clippy::has_significant_drop]
pub struct CdDrive {
    path: PathBuf,
    handle: HANDLE,
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
    #[cfg(target_family = "windows")]
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path: PathBuf = PathBuf::from(path.as_ref());
        let path_str = path.display().to_string();

        let _error = tracing::error_span!("CdDrive::open", path = %path_str).entered();

        let windrive = format!(r"\\.\{}", path.display());
        #[expect(unsafe_code, reason = "ffi call")]
        #[expect(
            clippy::multiple_unsafe_ops_per_block,
            reason = "embedded call to ensure raw pointer dropped immediately"
        )]
        let handle: HANDLE = unsafe {
            // SAFETY:
            // - All parameter values constructed with provided consts, no magic numbers used
            // - lpfilename (passed as raw pointer) is valid for duration of this block
            // - See https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfile2

            let lpfilename = WinString::from(windrive.as_str());
            let dwdesiredaccess = GENERIC_READ;
            let dwsharemode = FILE_SHARE_READ;
            let dwcreationdisposition = OPEN_EXISTING;

            CreateFile2(
                lpfilename.as_pcwstr(),
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

        let toc_command = CDROM_READ_TOC_EX {
            SessionTrack: 1,
            ..Default::default()
        };

        let mut toc = CDROM_TOC::default();
        let mut bytes_read: u32 = 0;

        #[expect(unsafe_code, reason = "ffi call")]
        let read_toc = unsafe {
            // SAFETY: inline based on
            // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_read_toc_ex
            DeviceIoControl(
                // valid handle - we have just created it
                handle,
                IOCTL_CDROM_READ_TOC_EX,
                // points to a buffer of type CDROM_READ_TOC_EX
                &toc_command as *const _ as *const _,
                // indicates the size, in bytes, of the input buffer,
                // which must be >= sizeof(CDROM_READ_TOC_EX).
                size_of_val(&toc_command) as u32,
                // CDROM_READ_TOC_EX does not allow setting `Format` but
                // `CDROM_READ_TOC_EX_FORMAT_TOC` is `0` (default) whereby
                // The output data is reported in a CDROM_TOC structure.
                &mut toc as *mut _ as *mut _,
                size_of_val(&toc) as u32,
                &mut bytes_read as *mut _,
                null_mut(),
            )
        };
        if read_toc == 0 {
            let error = io::Error::last_os_error();
            tracing::error!(name:"reading TOC", bytes_read, %error);

            #[expect(unsafe_code, reason = "ffi call")]
            unsafe {
                // SAFETY: handle has not been closed or mutated since it was opened above
                CloseHandle(handle as *mut _)
            };

            return Err(error);
        };
        assert!(bytes_read <= TOC_SIZE as u32);
        Ok(Self { path, handle, toc })
    }

    /// The path of the drive
    pub fn path(&self) -> &Path {
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
        &self.handle
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

    #[cfg(target_family = "windows")]
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
            TrackMode: TRACK_MODE_CDDA,
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
                IOCTL_CDROM_RAW_READ,
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

#[cfg(target_family = "windows")]
impl Drop for CdDrive {
    fn drop(&mut self) {
        #[expect(unsafe_code, reason = "ffi call")]
        unsafe {
            // SAFETY: handle
            // - was opened and validated in `open()`
            // - has not been closed (no such methods provided on Self)
            // - has not been externally mutated (no such methods provided on Self)
            CloseHandle(self.handle as *mut _);
        }
        // Not checking for success: cannot meaningfully handle CloseHandle failure during drop
    }
}

/// A pseudo-sector on an AudioCd
///
/// Windows DeviceIoControl wants offsets which pretend a [FRAME_SIZE]-byte frame is a 2048-byte
/// sector.
///
/// Internally stores the relative frame (excluding 150 lead-in frames)
pub struct Sector(i64);

impl Sector {
    /// Construct from an absolute frame number (including lead-in)
    pub fn from_frame(frame: Frame) -> Self {
        Self(frame.relative_to_leadin().as_usize() as i64)
    }

    /// For passing to `DeviceIoControl(..,IOCTL_CDROM_RAW_READ,..)`
    ///
    /// - Pretends that each frame is a 2048-byte sector.
    /// - Returned offset is relative to start of audio data
    pub fn offset(&self) -> i64 {
        self.0 * 2048
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A somewhat sane way of dealing with `PWSTR/PCWSTR`: A pointer to a null terminated string
/// consisting of 'wide chars' (u16), encoded using UTF-16.
///
/// Construct via `WinString::from(&str)`
pub struct WinString {
    words: Vec<u16>,
}

impl From<&str> for WinString {
    fn from(utf8: &str) -> Self {
        // see https://kennykerr.ca/rust-getting-started/string-tutorial.html
        let words = utf8.encode_utf16().chain(Some(0)).collect();
        Self { words }
    }
}

impl WinString {
    /// Create a `PCWSTR` - note this is a raw pointer.
    ///
    /// # SAFETY
    /// You must ensure that the returned `PCWSTR` is not used after self is dropped.
    /// It is recommended to call this directly in the call to a WinAPI unsafe function,
    /// see [AudioCd::new()] for an example
    #[expect(unsafe_code, reason = "returns raw pointer")]
    pub unsafe fn as_pcwstr(&self) -> PCWSTR {
        self.words.as_ptr()
    }
}
