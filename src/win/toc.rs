//! Windows-specific audio CD Table of Contents handling.
//!
//! Glues together:
//! - [`cdtoc::Toc`]
//! - windows ffi [`CDROM_TOC`]
//! - windows [`CdaFile`]s

use std::{fs, io, path::Path, ptr::null_mut};

use cdtoc::{Toc, TocError};
use tracing_result::Trace;

pub(crate) use super::bindings::CDROM_TOC;
use super::bindings::{CDROM_READ_TOC_EX, IOCTL_CDROM_READ_TOC_EX, TRACK_DATA};
use crate::{Frame, LEADIN, Msf, TocEntry, Track};

#[cfg(target_family = "windows")]
use super::{bindings::DeviceIoControl, drive::DriveHandle};

/// size of ffi struct [`CDROM_TOC`]
pub const TOC_SIZE: usize = size_of::<CDROM_TOC>();

/// size of `.cda` files
pub const CDA_LEN: usize = 0x2c;

impl CDROM_TOC {
    /// Load from disc
    #[cfg(target_family = "windows")]
    pub fn read_from(handle: &mut DriveHandle) -> io::Result<CDROM_TOC> {
        let toc_command = CDROM_READ_TOC_EX {
            SessionTrack: 1,
            ..Default::default()
        };

        let mut toc = CDROM_TOC::default();
        let mut bytes_read: u32 = 0;

        #[expect(unsafe_code, reason = "ffi call")]
        #[expect(clippy::multiple_unsafe_ops_per_block, reason = "obtain HANDLE inline")]
        let read_toc = unsafe {
            // SAFETY: inline based on
            // https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddcdrm/ni-ntddcdrm-ioctl_cdrom_read_toc_ex
            DeviceIoControl(
                // valid handle - upheld by DriveHandle
                *handle.as_handle_mut() as *mut _,
                const { IOCTL_CDROM_READ_TOC_EX.strict_cast_unsigned() },
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
        let _error_span = tracing::error_span!("reading TOC", bytes_read).entered();
        (read_toc != 0)
            .ok_or_else(io::Error::last_os_error)
            .or_error("")?;
        Ok(toc)
    }

    /// Parse raw bytes as a [`CDROM_TOC`] structure
    ///
    /// # Panics
    /// Will panic if `bytes` are not of correct size or alignment for a [`CDROM_TOC`]
    pub fn from_raw_bytes(bytes: Vec<u8>) -> CDROM_TOC {
        #[expect(unsafe_code, reason = "construction from raw bytes")]
        unsafe {
            // SAFETY: correct size & alignment
            assert_eq!(size_of::<CDROM_TOC>(), bytes.len());
            assert_eq!(0, bytes.as_ptr().align_offset(align_of::<CDROM_TOC>()));

            *(bytes.as_ptr() as *const _)
        }
    }

    pub fn as_toc(&self) -> Result<Toc, TocError> {
        let audio = self
            .iter_audio()
            .map(|entry| entry.start.as_usize() as u32)
            .collect();
        let leadout = self.leadout()?.as_usize() as u32;
        Toc::from_parts(audio, None, leadout)
    }

    pub fn iter_audio(&self) -> impl Iterator<Item = TocEntry> {
        self.TrackData
            .iter()
            .filter(|track| (1..0xA0).contains(&track.TrackNumber))
            .map(TocEntry::from)
    }

    /// The absolute start of the lead out
    pub fn leadout(&self) -> Result<Frame, TocError> {
        self.TrackData
            .iter()
            .find(|track| track.TrackNumber == 170)
            .map(TocEntry::from)
            .map(|entry| entry.start)
            .ok_or(TocError::SectorOrder)
    }
}

impl From<&TRACK_DATA> for TocEntry {
    /// Creates a [`TocEntry`] from Windows API CDROM_TRACK_DATA.
    ///
    /// # Arguments
    ///
    /// * `track_data` - Raw track data from the Windows CDROM_TOC
    ///
    /// # Notes
    ///
    /// - The address is read as big-endian and converted to a frame position
    /// - The lead-in offset is added to get the absolute frame position
    ///
    /// # TODOs
    ///
    /// - Consider making this fallible with `TryFrom` for better error handling
    fn from(track_data: &TRACK_DATA) -> Self {
        let relative = u32::from_be_bytes(track_data.Address);
        let start = Frame::new(relative as usize) + LEADIN;
        let track = track_data.TrackNumber;
        Self { track, start }
    }
}

/// A windows .cda file detailling CD TOC info for a given track.
///
/// See https://en.wikipedia.org/wiki/.cda_file
pub struct CdaFile {
    /// The first track has the number 1
    track_number: u16,
    /// Identifier calculated by Windows for cdplayer.exe
    windows_identifier: u32,
    /// *Absolute* start position: minutes, seconds, frames. Including
    /// the 2s lead-in. (Track 1 is usually at 0,2,0)
    ///
    /// ### Note:
    /// For inexplicable, probably historical, reasons Windows stores the *absolute* time
    /// and the *relative* frame in cda files. By storing `start` as [`Msf`] we aim to avoid
    /// any confusion as we can store the *absolute* value without diverging from the
    /// file contents.
    start: Msf,
    /// Duration of the track: minutes, seconds, frames
    duration: Msf,
}

impl CdaFile {
    /// Read the `.cda` file at `path`
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let path = path.as_ref();
        let _warn = tracing::warn_span!("reading Cda from path", path = %path.display()).entered();

        let raw = fs::read(path)?;
        Self::new(raw)
    }

    /// Create a new `CdaFile`
    ///
    /// Contents are validated based on https://en.wikipedia.org/wiki/.cda_file
    pub fn new(contents: Vec<u8>) -> io::Result<Self> {
        let _trace = tracing::trace_span!("CdaFile::new", ?contents).entered();

        let mut data = contents.iter().copied();

        // After this check it's OK to call `unwrap` on `next_chunk`
        (data.len() == CDA_LEN)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "invalid CDA file: expected {} bytes, got {}",
                        CDA_LEN,
                        contents.len()
                    ),
                )
            })
            .or_warn("")?;

        (&data.next_chunk().unwrap() == b"RIFF")
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Missing or invalid RIFF header")
            })
            .or_warn("")?;

        let chunk_size = u32::from_le_bytes(data.next_chunk().unwrap());
        (chunk_size == 36)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid chunk size: expected 36, got {}", chunk_size),
                )
            })
            .or_warn("")?;

        (&data.next_chunk().unwrap() == b"CDDA")
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Missing or invalid CDDA identifier",
                )
            })
            .or_warn("")?;

        (&data.next_chunk().unwrap() == b"fmt ")
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Missing or invalid fmt identifier",
                )
            })
            .or_warn("")?;

        let chunk_size = u32::from_le_bytes(data.next_chunk().unwrap());
        (chunk_size == 24)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("Invalid chunk size: expected 24, got {}", chunk_size),
                )
            })
            .or_warn("")?;

        let version = u16::from_le_bytes(data.next_chunk().unwrap());
        (version == 1)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown cda file version {}", version),
                )
            })
            .or_warn("")?;

        let track_number = u16::from_le_bytes(data.next_chunk().unwrap());
        let windows_identifier = u32::from_le_bytes(data.next_chunk().unwrap());

        // For inexplicable, probably historical, reasons Windows stores the relative frame in cda
        let starting_frame =
            Frame(u32::from_le_bytes(data.next_chunk().unwrap()) as usize) + LEADIN;
        let duration_frames = Frame(u32::from_le_bytes(data.next_chunk().unwrap()) as usize);

        let start = Msf {
            frame: data.next().unwrap(),
            sec: data.next().unwrap(),
            min: data.next().unwrap(),
        };
        (start == starting_frame)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("mismatched start: frame {starting_frame:?} / {start:?}"),
                )
            })
            .or_warn("")?;

        (data.next().unwrap() == 0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing null byte"))
            .or_warn("")?;

        let duration = Msf {
            frame: data.next().unwrap(),
            sec: data.next().unwrap(),
            min: data.next().unwrap(),
        };
        (duration == duration_frames)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("mismatched duration {duration_frames:?} frames / {duration:?}"),
                )
            })
            .or_warn("")?;

        (data.next().unwrap() == 0)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing null byte"))
            .or_warn("")?;

        Ok(Self {
            track_number,
            windows_identifier,
            start,
            duration,
        })
    }
}

impl From<CdaFile> for Track<'static> {
    fn from(cda: CdaFile) -> Self {
        Self {
            toc_entry: TocEntry {
                track: cda.track_number as u8,
                start: cda.start.into(),
            },
            duration: cda.duration.into(),
            windows_identifier: Some(cda.windows_identifier),
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::albums::TestAlbum;
    use glob::glob;
    use rstest::rstest;

    #[rstest]
    #[case(TestAlbum::DefinitelyMaybe)]
    #[case(TestAlbum::TheWallDisc1)]
    #[case(TestAlbum::TheWallDisc2)]
    fn toc_basic_properties(#[case] album: TestAlbum) {
        let toc = album.load_cdrom_toc();
        assert_eq!(toc.FirstTrack, album.expected_first_track());
        assert_eq!(toc.LastTrack, album.expected_last_track());
    }

    #[rstest]
    #[case(TestAlbum::DefinitelyMaybe)]
    #[case(TestAlbum::TheWallDisc1)]
    #[case(TestAlbum::TheWallDisc2)]
    fn leadout(#[case] album: TestAlbum) {
        let toc = album.load_cdrom_toc();
        let leadout = toc.leadout().unwrap();
        assert_eq!(leadout, album.expected_leadout());
    }

    #[rstest]
    #[case(TestAlbum::DefinitelyMaybe)]
    #[case(TestAlbum::TheWallDisc1)]
    #[case(TestAlbum::TheWallDisc2)]
    fn audio(#[case] album: TestAlbum) {
        let toc = album.load_cdrom_toc();
        let audio_tracks: Vec<TocEntry> = toc.iter_audio().collect();
        assert_eq!(audio_tracks, album.expected_toc_entries());
    }

    #[rstest]
    #[case(TestAlbum::DefinitelyMaybe)]
    #[case(TestAlbum::TheWallDisc1)]
    #[case(TestAlbum::TheWallDisc2)]
    fn parse_cdas(#[case] album: TestAlbum) {
        let expected_tracks = album.expected_tracks_minimal();
        let cdas = album.assets_path().join("*.cda");
        let cdas: Vec<_> = glob(&cdas.to_string_lossy()).unwrap().collect();

        assert_eq!(cdas.len(), expected_tracks.len());

        for (cda_file, track) in cdas.into_iter().zip(expected_tracks) {
            let cda = CdaFile::from_path(cda_file.unwrap()).unwrap();
            assert_eq!(cda.track_number as u8, track.track_number());
            assert_eq!(cda.start, track.toc_entry.start);
            assert_eq!(cda.duration, track.duration);
            // Cannot check windows_identifier without making _minimal more than minimal
            // assert_eq!(Some(cda.windows_identifier), track.windows_identifier);
        }
    }

    #[rstest]
    #[case(TestAlbum::DefinitelyMaybe)]
    #[case(TestAlbum::TheWallDisc1)]
    #[case(TestAlbum::TheWallDisc2)]
    fn compare_toc(#[case] album: TestAlbum) {
        let cdrom_toc = album.load_cdrom_toc();
        let toc = album.expected_toc();

        assert_eq!(toc, cdrom_toc.as_toc().unwrap())
    }
}
