//! Handling of windows [`CdaFile`]s: [specification](https://en.wikipedia.org/wiki/.cda_file).

use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

use tracing_result::Trace;

use crate::{Frame, LEADIN, Track, frame::Msf, toc::TrackNo};

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
    ///
    /// Concretely the file stores the start twice: at 0x1c as a *relative* frame offset (LBN,
    /// where 0 is the start of the program area = 00:02:00 absolute - Windows works in LBN
    /// internally and merely adds the fixed 150-frame lead-in when rendering MSF), and at 0x24
    /// as the *absolute* MSF taken from the disc TOC. [`CdaFile::new`] cross-checks the two
    /// representations, so a well-formed file always has `start >= 00:02:00` (frame 150). The
    /// exception is deliberate copy-protection breakage (e.g. a track at 00:01:74): the MSF is
    /// kept, with a warning, instead of failing the cross-check.
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
        // files (0x1c) alongside the absolute MSF (0x24). Normally they agree:
        // relative frame + lead-in == absolute MSF, and we cross-check them below.
        let relative_frame = u32::from_le_bytes(data.next_chunk().unwrap());
        let duration_frames = Frame::new(u32::from_le_bytes(data.next_chunk().unwrap()))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "duration too large"))
            .or_warn("parsing duration")?;

        let start_frame = data.next().unwrap();
        let start_sec = data.next().unwrap();
        let start_min = data.next().unwrap();
        let start = Msf::new(start_min, start_sec, start_frame);

        let msf_frames = Frame::from(start);
        let relative_start = Frame::new(relative_frame).and_then(|frame| frame.checked_add(LEADIN));

        // Escape hatch for deliberate TOC breakage (copy protection): "More Music from The Fast
        // and the Furious" (CP-2 in the cdcopy-drm02 paper) has a track start of 00:01:74, below
        // the lead-in, which has no valid relative-frame representation. The MSF straight from
        // the disc TOC is the more viable value there, so warn and keep it instead of failing.
        let start_below_leadin = msf_frames < LEADIN;
        let _warn_issued = (start_below_leadin && relative_start != Some(msf_frames)).then(|| {
            tracing::warn!(
                name: "cda start below the lead-in; trusting the MSF over the relative frame",
                relative_frame,
                ?start
            )
        });

        (relative_start == Some(msf_frames) || start_below_leadin)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("mismatched start: relative frame {relative_frame} / msf {start:?}"),
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

impl TryFrom<CdaFile> for Track<'static> {
    type Error = io::Error;

    fn try_from(cda: CdaFile) -> Result<Self, Self::Error> {
        let track = TrackNo::new(cda.track_number)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    format!("invalid track number {} in cda file", cda.track_number),
                )
            })
            .or_warn("cda file does not describe a normal audio track")?;
        Ok(Track::new(track, cda.start.into(), cda.duration.into()))
    }
}

/// Parse all the cda files at `path` into [`Track`]s
///
/// These are guaranteed to be sorted as glob yields paths in alphabetical order
///
/// # Notes
/// - Only fatal errors are returned, e.g. for an invalid path. `.cda` files which fail to parse,
///   or which do not describe a normal audio track, are skipped with a warning rather than failing
///   the whole read.
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
    let tracks = cda_files.filter_map(|cda| {
        Track::try_from(cda)
            .or_warn("cda file does not describe a normal audio track")
            .ok()
    });
    Ok(tracks.collect())
}

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {
    use super::*;

    use std::assert_matches;

    use crate::test_fixtures::albums::TestAlbum::{self, *};
    use glob::glob;
    use rstest::rstest;
    use tracing_test::traced_test;

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
            assert_eq!(TrackNo::new(cda.track_number), Some(track.track_number()));
            assert_eq!(cda.start, track.toc_entry.start());
            assert_eq!(cda.duration, track.duration);
            assert_matches!(Track::try_from(cda), Ok(_));
            // Cannot check windows_identifier without making _minimal more than minimal
            // assert_eq!(Some(cda.windows_identifier), track.windows_identifier);
        }
    }

    #[traced_test]
    #[rstest]
    #[case(0)]
    #[case(u32::MAX)]
    fn start_below_leadin_is_kept_with_a_warning(#[case] relative_frame: u32) {
        // Deliberate TOC breakage (copy protection): "More Music from The Fast and the Furious"
        // (CP-2 in the cdcopy-drm02 paper) has a track start of 00:01:74, below the lead-in.
        let cda_file = DefinitelyMaybe.assets_path().join("Track01.cda");
        let mut contents = std::fs::read(&cda_file).unwrap();

        // absolute MSF at 0x24 (frame, sec, min) = 00:01:74 (fields are binary, not BCD)
        contents[0x24] = 0x4A;
        contents[0x25] = 0x01;
        contents[0x26] = 0x00;

        // the relative frame at 0x1c has no valid representation for a start below the lead-in:
        // Windows either clamps to 0 or wraps -1 to u32::MAX; both must parse, keeping the MSF
        contents[0x1c..0x20].copy_from_slice(&relative_frame.to_le_bytes());
        let cda = CdaFile::new(contents.clone()).unwrap();
        assert_eq!(cda.start, Msf::new(0x00, 0x01, 0x4A));

        // the escape hatch warns instead of failing; the warning's text is the event name,
        // which the fmt layer doesn't print, so assert on the level & fields instead
        assert!(logs_contain("WARN"));
        assert!(logs_contain(&format!("relative_frame={relative_frame}")));

        // a genuine mismatch at/above the lead-in still fails
        contents[0x1c..0x20].copy_from_slice(&0_u32.to_le_bytes());
        contents[0x24] = 0x00;
        contents[0x25] = 0x03;
        contents[0x26] = 0x00;
        assert!(CdaFile::new(contents).is_err());
    }

    #[rstest]
    #[case(crate::toc::b0000::LEADOUT_TRACK_ID.cast::<u16>())]
    #[case(0)]
    #[case(TrackNo::MAX.cast::<u16>() + 1)]
    fn try_from_rejects_special_track_numbers(#[case] track_number: u16) {
        let album = DefinitelyMaybe;
        let cdas = album.assets_path().join("*.cda");
        let cda_file = glob(&cdas.to_string_lossy())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();

        let mut cda = CdaFile::from_path(&cda_file).unwrap();
        cda.track_number = track_number;
        let err = Track::try_from(cda).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidData);
    }
}
