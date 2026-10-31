//! Handling of windows [`CdaFile`]s: [specification](https://en.wikipedia.org/wiki/.cda_file).

use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

use tracing_result::Trace;

use crate::{Frame, LEADIN, Track, frame::Msf, toc::TocEntry};

/// size of `.cda` files
pub const CDA_LEN: usize = 0x2c;

/// A windows .cda file detailling CD TOC info for a given track.
///
/// See [specification](https://en.wikipedia.org/wiki/.cda_file)
pub struct CdaFile {
    /// The first track has the number 1
    track_number: u16,
    /// Identifier calculated by Windows for cdplayer.exe
    _windows_identifier: u32,
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
    /// Contents are validated based on [specification](https://en.wikipedia.org/wiki/.cda_file)
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
            Frame::new(u32::from_le_bytes(data.next_chunk().unwrap())).strict_add(LEADIN);
        let duration_frames = Frame::new(u32::from_le_bytes(data.next_chunk().unwrap()));

        let start_frame = data.next().unwrap();
        let start_sec = data.next().unwrap();
        let start_min = data.next().unwrap();
        let start = Msf::new(start_min, start_sec, start_frame);

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

        let frame = data.next().unwrap();
        let sec = data.next().unwrap();
        let min = data.next().unwrap();
        let duration = Msf::new(min, sec, frame);

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
            _windows_identifier: windows_identifier,
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
                control: Default::default(),
            },
            duration: cda.duration.into(),
            ..Default::default()
        }
    }
}

/// Parse all the cda files at `path` into [`Track`]s
///
/// These are guaranteed to be sorted as glob yields paths in alphabetical order
pub fn tracks<P: AsRef<Path>>(path: &P) -> io::Result<Vec<Track<'_>>> {
    let glob_path = path.as_ref().join("*.cda");
    let glob_pattern = glob_path
        .to_str()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidFilename, "drive path not valid"))
        .or_error("drive path contains non utf-8")?;
    let paths = glob::glob(glob_pattern)
        .map_err(|err| io::Error::new(ErrorKind::InvalidFilename, err))
        .or_error("invalid glob pattern")?;
    let valid_paths = paths.filter_map(|cda| {
        cda.map_err(|error| io::Error::new(ErrorKind::InvalidFilename, error))
            .or_error("unable to access globbed file")
            .ok()
    });
    let cda_files =
        valid_paths.filter_map(|path| CdaFile::from_path(path).or_warn("invalid cda file").ok());
    let tracks = cda_files.map(Track::from);
    Ok(tracks.collect())
}

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {
    use super::*;
    use crate::test_fixtures::albums::TestAlbum::{self, *};
    use glob::glob;
    use rstest::rstest;

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    fn parse_cdas(#[case] album: TestAlbum) {
        let expected_tracks = album.expected_audio_tracks_minimal();
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
}
