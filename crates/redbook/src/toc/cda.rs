//! Handling of windows [`CdaFile`]s: [specification](https://en.wikipedia.org/wiki/.cda_file).

use std::{fs, path::Path};

use bytes::Buf;
use tracing_result::Trace;

use crate::{
    Frame, LEADIN,
    disc::Track,
    frame::Msf,
    toc::{TrackNo, error::CdaError},
};

/// size of `.cda` files
pub const CDA_LEN: usize = 0x2c;

/// A windows .cda file detailling CD TOC info for a given track.
///
/// See [specification](https://en.wikipedia.org/wiki/.cda_file)
#[derive(Debug, Clone, Copy)]
pub struct CdaFile {
    /// The first track has the number 1
    track_number: TrackNo,
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
    /// Create a new `CdaFile`
    ///
    /// Contents are validated based on [specification](https://en.wikipedia.org/wiki/.cda_file)
    pub fn new<BYTES: AsRef<[u8]>>(contents: BYTES) -> Result<Self, CdaError> {
        let mut data = contents.as_ref();
        let input_len = data.len();
        let _trace = tracing::trace_span!("CdaFile::new", ?data).entered();

        let magic = data.try_get_u32_le()?.to_le_bytes();
        (magic == *b"RIFF")
            .ok_or(CdaError::InvalidHeader(magic))
            .or_warn("")?;

        let chunk_size = data.try_get_u32_le()?;
        (chunk_size == 36)
            .ok_or(CdaError::InvalidChunkSize {
                expected: 36,
                actual: chunk_size,
            })
            .or_warn("")?;

        let magic = data.try_get_u32_le()?.to_le_bytes();
        (magic == *b"CDDA")
            .ok_or(CdaError::InvalidHeader(magic))
            .or_warn("")?;

        let magic = data.try_get_u32_le()?.to_le_bytes();
        (magic == *b"fmt ")
            .ok_or(CdaError::InvalidHeader(magic))
            .or_warn("")?;

        let chunk_size = data.try_get_u32_le()?;
        (chunk_size == 24)
            .ok_or(CdaError::InvalidChunkSize {
                expected: 24,
                actual: chunk_size,
            })
            .or_warn("")?;

        let version = data.try_get_u16_le()?;
        (version == 1)
            .ok_or(CdaError::InvalidVersion(version))
            .or_warn("")?;

        let track_number = TrackNo::new(data.try_get_u16_le()?)
            .or_warn("cda file does not describe a normal audio track")?;
        let windows_identifier = data.try_get_u32_le()?;

        // For inexplicable, probably historical, reasons Windows stores the relative frame in cda
        // files (0x1c) alongside the absolute MSF (0x24). Normally they agree:
        // relative frame + lead-in == absolute MSF, and we cross-check them below.
        let relative_frame = data.try_get_u32_le()?;
        let duration_frames = Frame::new(data.try_get_u32_le()?).or_warn("parsing duration")?;

        let start_frame = data.try_get_u8()?;
        let start_sec = data.try_get_u8()?;
        let start_min = data.try_get_u8()?;
        let start = Msf::new(start_min, start_sec, start_frame);

        let msf_frames = Frame::from(start);
        let relative_start = Frame::new(relative_frame)
            .ok()
            .and_then(|frame| frame.checked_add(LEADIN));

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
            .ok_or(CdaError::MismatchedStart {
                relative: relative_frame,
                absolute: start,
            })
            .or_warn("")?;

        (data.try_get_u8()? == 0)
            .ok_or(CdaError::MissingNullByte)
            .or_warn("")?;

        let frame = data.try_get_u8()?;
        let sec = data.try_get_u8()?;
        let min = data.try_get_u8()?;
        let duration = Msf::new(min, sec, frame);

        (duration == duration_frames)
            .ok_or(CdaError::MismatchedDuration {
                frames: duration_frames,
                msf: duration,
            })
            .or_warn("")?;

        (data.try_get_u8()? == 0)
            .ok_or(CdaError::MissingNullByte)
            .or_warn("")?;

        // Short files already failed above via `From<TryGetError>`; this catches trailing
        // bytes in over-long files so the length must be exactly CDA_LEN
        (data.remaining() == 0)
            .ok_or(CdaError::InvalidLength {
                expected: CDA_LEN,
                actual: input_len,
            })
            .or_warn("")?;

        Ok(Self {
            track_number,
            windows_identifier,
            start,
            duration,
        })
    }

    /// Read the `.cda` file at `path`
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self, CdaError> {
        let path = path.as_ref();
        let _warn = tracing::warn_span!("reading Cda from path", path = %path.display()).entered();

        let raw = fs::read(path)?;
        Self::new(raw)
    }

    /// The track number: the first track has the number 1 (always within 1..=[`TrackNo::MAX`])
    pub fn track_number(&self) -> TrackNo {
        self.track_number
    }

    /// Identifier calculated by Windows for cdplayer.exe
    pub fn windows_identifier(&self) -> u32 {
        self.windows_identifier
    }

    /// *Absolute* start position: minutes, seconds, frames. Including
    /// the 2s lead-in. (Track 1 is usually at 0,2,0)
    pub fn start(&self) -> Msf {
        self.start
    }

    /// Duration of the track: minutes, seconds, frames
    pub fn duration(&self) -> Msf {
        self.duration
    }
}

impl From<CdaFile> for Track<'static> {
    fn from(cda: CdaFile) -> Self {
        Track::new(
            cda.track_number(),
            cda.start().into(),
            cda.duration().into(),
        )
    }
}

/// Parse all the cda files at `path` into [`Track`]s
///
/// These are guaranteed to be sorted as glob yields paths in alphabetical order
///
/// # Notes
/// - Only fatal errors are returned, e.g. for an invalid path. `.cda` files which fail to parse
///   (including those which do not describe a normal audio track) are skipped with a warning
///   rather than failing the whole read.
pub fn tracks<P: AsRef<Path>>(path: &P) -> Result<Vec<Track<'_>>, CdaError> {
    let glob_path = path.as_ref().join("*.cda");
    let glob_pattern = glob_path
        .to_str()
        .ok_or_else(|| CdaError::InvalidFilename(glob_path.clone()))
        .or_error("drive path contains non utf-8")?;
    let paths = glob::glob(glob_pattern)
        .map_err(|_| CdaError::InvalidFilename(glob_path.clone()))
        .or_error("invalid glob pattern")?;
    let valid_paths = paths.filter_map(|cda| cda.or_error("unable to access globbed file").ok());
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
            assert_eq!(cda.track_number(), track.trackno());
            assert_eq!(cda.start(), track.start());
            assert_eq!(cda.duration(), track.duration());
            assert_eq!(Track::from(cda), track);
            // Cannot check windows_identifier without making _minimal more than minimal
            // assert_eq!(cda.windows_identifier(), track.windows_identifier);
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
        assert_eq!(cda.start(), Msf::new(0x00, 0x01, 0x4A));

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
    #[case(100)]
    #[case(0)]
    #[case(TrackNo::MAX.cast::<u16>() + 1)]
    fn new_rejects_special_track_numbers(#[case] track_number: u16) {
        let album = DefinitelyMaybe;
        let cdas = album.assets_path().join("*.cda");
        let cda_file = glob(&cdas.to_string_lossy())
            .unwrap()
            .next()
            .unwrap()
            .unwrap();

        let mut contents = std::fs::read(&cda_file).unwrap();
        // the track number is stored as a u16 LE at offset 0x16
        contents[0x16..0x18].copy_from_slice(&track_number.to_le_bytes());
        let err = CdaFile::new(contents).unwrap_err();
        assert!(matches!(err, CdaError::InvalidTrackNo(_)));
    }

    #[rstest]
    fn trailing_bytes_report_full_input_length() {
        let cda_file = DefinitelyMaybe.assets_path().join("Track01.cda");
        let mut contents = std::fs::read(&cda_file).unwrap();
        contents.extend_from_slice(&[0xFF; 4]);

        let err = CdaFile::new(contents.clone()).unwrap_err();
        assert!(matches!(
            err,
            CdaError::InvalidLength {
                expected: CDA_LEN,
                actual
            } if actual == contents.len()
        ));
    }
}
