//! Windows-specific audio CD Table of Contents handling.
//!
//! Glues together:
//! - [`cdtoc::Toc`]
//! - windows ffi [`CDROM_TOC`]
//! - windows [`CdaFile`]s

use std::{fs, io, path::Path, time::Duration};

use cdtoc::{Toc, TocError};
use tracing_result::Trace;
use windows_sys::Win32::Devices::Cdrom::CDROM_TOC;

use crate::{Frame, LEADIN, Msf, TocEntry, Track};

/// size of ffi struct [`CDROM_TOC`]
pub const TOC_SIZE: usize = size_of::<CDROM_TOC>();

/// size of `.cda` files
pub const CDA_LEN: usize = 0x2c;

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

/// A windows .cda file detailling CD TOC info for a given track.
///
/// See https://en.wikipedia.org/wiki/.cda_file
pub struct CdaFile {
    /// Should always be 1.
    version: u16,
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
            version,
            track_number,
            windows_identifier,
            start,
            duration,
        })
    }
}

impl TryFrom<CdaFile> for Track<'static> {
    type Error = io::Error;

    fn try_from(cda: CdaFile) -> Result<Self, Self::Error> {
        todo!();
        // let data = cda.raw;
        // const MIN_LEN: usize = 44;
        // if data.len() < MIN_LEN {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         format!(
        //             "CDA file too short: expected at least {} bytes, got {}",
        //             MIN_LEN,
        //             data.len()
        //         ),
        //     ));
        // }

        // // Validate RIFF header
        // if &data[0..4] != b"RIFF" {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         "Missing or invalid RIFF header",
        //     ));
        // }

        // // Validate chunk size (always 36)
        // let chunk_size = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        // if chunk_size != 36 {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         format!("Invalid chunk size: expected 36, got {}", chunk_size),
        //     ));
        // }

        // // Validate CDDA identifier
        // if &data[8..12] != b"CDDA" {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         "Missing or invalid CDDA identifier",
        //     ));
        // }

        // // Validate fmt chunk identifier
        // if &data[12..16] != b"fmt " {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         "Missing or invalid fmt chunk identifier",
        //     ));
        // }

        // // Validate fmt chunk size (always 24)
        // let fmt_chunk_size = u32::from_le_bytes([data[16], data[17], data[18], data[19]]);
        // if fmt_chunk_size != 24 {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         format!(
        //             "Invalid fmt chunk size: expected 24, got {}",
        //             fmt_chunk_size
        //         ),
        //     ));
        // }

        // // Parse version (always 1)
        // let version = u16::from_le_bytes([data[0x14], data[0x15]]);
        // if version != 1 {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         format!("Invalid version: expected 1, got {}", version),
        //     ));
        // }

        // let track_number = u16::from_le_bytes([data[0x16], data[0x17]]);
        // let windows_identifier = Some(u32::from_le_bytes([
        //     data[0x18], data[0x19], data[0x1A], data[0x1B],
        // ]));
        // let range_offset_frames =
        //     u32::from_le_bytes([data[0x1C], data[0x1D], data[0x1E], data[0x1F]]);
        // // For inexplicable, probably historical, reasons Windows stores the relative frame in cda
        // let starting_frame = Frame(range_offset_frames as usize) + LEADIN;
        // let duration_frames = u32::from_le_bytes([data[0x20], data[0x21], data[0x22], data[0x23]]);
        // let duration_frames = Frame(duration_frames as usize);

        // let starting_time = Msf {
        //     frame: data[0x24],
        //     sec: data[0x25],
        //     min: data[0x26],
        // };
        // // For inexplicable, probably historical, reasons Windows stores the absolute time in cda
        // let starting_time = starting_time - Duration::from_secs(2);

        // // Validate null byte after range position
        // if data[0x27] != 0 {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         "Expected null byte after range position",
        //     ));
        // }

        // // Parse duration time
        // let duration = Msf {
        //     frame: data[0x28],
        //     sec: data[0x29],
        //     min: data[0x2A],
        // };

        // // Validate null byte after duration
        // if data[0x2B] != 0 {
        //     return Err(io::Error::new(
        //         io::ErrorKind::InvalidData,
        //         "Expected null byte after duration",
        //     ));
        // }

        // // For inexplicable, probably historical, reasons Windows stores the
        // // *relative* frame and *absolute* time in cda
        // debug_assert_eq!(starting_frame.relative_to_leadin(), starting_time);
        // debug_assert_eq!(duration_frames, duration);

        // let toc_entry = TocEntry {
        //     track: track_number as u8,
        //     start: starting_frame,
        // };

        // Ok(Track {
        //     toc_entry,
        //     windows_identifier,
        //     duration: duration_frames,
        //     ..Default::default()
        // })
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
