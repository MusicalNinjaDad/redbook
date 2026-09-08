//! Windows-specific audio CD Table of Contents handling.
//! 
//! Glues together:
//! - [`cdtoc::Toc`]
//! - windows ffi [`CDROM_TOC`]
//! - windows [`CdaFile`]s

use std::{fs, io, path::Path, time::Duration};

use cdtoc::{Toc, TocError};
use windows_sys::Win32::Devices::Cdrom::CDROM_TOC;

use crate::{Frame, LEADIN, Msf, TocEntry, Track};

pub const TOC_SIZE: usize = size_of::<CDROM_TOC>();

/// Manipulation of [`CDROM_TOC`]
pub trait CdromTocExt {
    /// Parse raw bytes as a [`CDROM_TOC`] structure
    ///
    /// # Panics
    /// Will panic if `bytes` are not of correct size or alignment for a [`CDROM_TOC`]
    fn from_raw_bytes(bytes: Vec<u8>) -> CDROM_TOC;

    fn to_toc(&self) -> Result<Toc, TocError>;

    fn iter_audio(&self) -> impl Iterator<Item = TocEntry>;

    /// The absolute start of the lead out
    fn leadout(&self) -> Result<Frame, TocError>;
}

impl CdromTocExt for CDROM_TOC {
    fn from_raw_bytes(bytes: Vec<u8>) -> CDROM_TOC {
        #[expect(unsafe_code, reason = "construction from raw bytes")]
        unsafe {
            // SAFETY: correct size & alignment
            assert_eq!(size_of::<CDROM_TOC>(), bytes.len());
            assert_eq!(0, bytes.as_ptr().align_offset(align_of::<CDROM_TOC>()));

            *(bytes.as_ptr() as *const _)
        }
    }

    fn to_toc(&self) -> Result<Toc, TocError> {
        let audio = self
            .iter_audio()
            .map(|entry| entry.start.as_usize() as u32)
            .collect();
        let leadout = self.leadout()?.as_usize() as u32;
        Toc::from_parts(audio, None, leadout)
    }

    fn iter_audio(&self) -> impl Iterator<Item = TocEntry> {
        self.TrackData
            .iter()
            .filter(|track| (1..0xA0).contains(&track.TrackNumber))
            .map(TocEntry::from)
    }

    fn leadout(&self) -> Result<Frame, TocError> {
        self.TrackData
            .iter()
            .find(|track| track.TrackNumber == 170)
            .map(TocEntry::from)
            .map(|entry| entry.start)
            .ok_or(TocError::SectorOrder)
    }
}

/// A windows .cda file detailling CD TOC info for a given track
pub struct CdaFile {
    raw: Vec<u8>,
}

impl CdaFile {
    /// Read the `.cda` file at `path`
    pub fn from_path<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let raw = fs::read(&path)?;
        Ok(Self::new(raw))
    }

    /// Create a new `CdaFile`
    ///
    /// # TODO
    /// Validate contents
    pub fn new(contents: Vec<u8>) -> Self {
        Self { raw: contents }
    }
}

/// Parsing based on https://en.wikipedia.org/wiki/.cda_file
impl TryFrom<CdaFile> for Track<'static> {
    type Error = io::Error;

    fn try_from(cda: CdaFile) -> Result<Self, Self::Error> {
        let data = cda.raw;
        const MIN_LEN: usize = 44;
        if data.len() < MIN_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "CDA file too short: expected at least {} bytes, got {}",
                    MIN_LEN,
                    data.len()
                ),
            ));
        }

        // Validate RIFF header
        if &data[0..4] != b"RIFF" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing or invalid RIFF header",
            ));
        }

        // Validate chunk size (always 36)
        let chunk_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        if chunk_size != 36 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid chunk size: expected 36, got {}", chunk_size),
            ));
        }

        // Validate CDDA identifier
        if &data[8..12] != b"CDDA" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing or invalid CDDA identifier",
            ));
        }

        // Validate fmt chunk identifier
        if &data[12..16] != b"fmt " {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Missing or invalid fmt chunk identifier",
            ));
        }

        // Validate fmt chunk size (always 24)
        let fmt_chunk_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        if fmt_chunk_size != 24 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Invalid fmt chunk size: expected 24, got {}",
                    fmt_chunk_size
                ),
            ));
        }

        // Parse version (always 1)
        let version = u16::from_le_bytes([data[0x14], data[0x15]]);
        if version != 1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Invalid version: expected 1, got {}", version),
            ));
        }

        let track_number = u16::from_le_bytes([data[0x16], data[0x17]]);
        let windows_identifier = Some(u32::from_le_bytes([
            data[0x18], data[0x19], data[0x1A], data[0x1B],
        ]));
        let range_offset_frames =
            u32::from_le_bytes([data[0x1C], data[0x1D], data[0x1E], data[0x1F]]);
        // For inexplicable, probably historical, reasons Windows stores the relative frame in cda
        let starting_frame = Frame(range_offset_frames as usize) + LEADIN;
        let duration_frames = u32::from_le_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]);
        let duration_frames = Frame(duration_frames as usize);

        let starting_time = Msf {
            frame: data[0x24],
            sec: data[0x25],
            min: data[0x26],
        };
        // For inexplicable, probably historical, reasons Windows stores the absolute time in cda
        let starting_time = starting_time - Duration::from_secs(2);

        // Validate null byte after range position
        if data[0x27] != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected null byte after range position",
            ));
        }

        // Parse duration time
        let duration = Msf {
            frame: data[0x28],
            sec: data[0x29],
            min: data[0x2A],
        };

        // Validate null byte after duration
        if data[0x2B] != 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Expected null byte after duration",
            ));
        }

        // For inexplicable, probably historical, reasons Windows stores the
        // *relative* frame and *absolute* time in cda
        debug_assert_eq!(starting_frame.relative_to_leadin(), starting_time);
        debug_assert_eq!(duration_frames, duration);

        let toc_entry = TocEntry {
            track: track_number as u8,
            start: starting_frame,
        };

        Ok(Track {
            toc_entry,
            windows_identifier,
            duration: duration_frames,
            ..Default::default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixtures::albums::TestAlbum;
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
}
