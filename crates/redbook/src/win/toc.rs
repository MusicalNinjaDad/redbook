#![expect(missing_docs, reason = "WIP")]
//! Windows-specific audio CD Table of Contents handling.
//!
//! Glues together:
//! - [`cdtoc::Toc`]
//! - windows ffi [`CDROM_TOC`]
//! - windows [`CdaFile`]s

use std::{fs, io, path::Path};

use cdtoc::Toc;
use tracing_result::Trace;

pub use super::bindings::CDROM_TOC;
use super::bindings::TRACK_DATA;
use crate::{Control, Frame, LEADIN, Msf, TocEntries, TocEntry, Track};

/// size of `.cda` files
pub const CDA_LEN: usize = 0x2c;

/// size of ffi struct [`CDROM_TOC`]
pub const TOC_SIZE: usize = size_of::<CDROM_TOC>();

impl CDROM_TOC {
    /// Returns the full size, in bytes of the data-carrying part of the struct.
    ///
    /// While `Self.Length` indicates the length, in bytes, of the table of contents data, it does
    /// not include the length of the Length member itself. This function returns the total size,
    /// including the Length member.
    fn size(&self) -> usize {
        u16::from_be_bytes(self.Length)
            .strict_cast::<usize>()
            .strict_add(size_of_val(&self.Length))
    }

    /// Returns the number of actual entries in the TrackData array. This is calculated based
    /// upon the total size of the struct.
    pub fn track_entries(&self) -> usize {
        let entries_size = self
            .size()
            .strict_sub(size_of_val(&self.Length))
            .strict_sub(size_of_val(&self.FirstTrack))
            .strict_sub(size_of_val(&self.LastTrack));
        entries_size
            .div_exact(size_of::<TRACK_DATA>().strict_cast())
            .expect(
                "size used by `TrackDetails` must be exactly divisible by `size_of::<TRACK_DATA>`",
            )
    }

    /// Returns the track number of the last audio track, or `None` if no audio tracks are present.
    pub fn last_audio_track(&self) -> Option<usize> {
        self.TrackData
            .iter()
            .take(self.LastTrack.strict_cast())
            .rfind(|track| !track.control().contains(Control::DATA_TRACK))
            .map(|track| track.TrackNumber.strict_cast())
    }

    /// Parse raw bytes as a [`CDROM_TOC`] structure. This expects the relevant bytes
    /// and will pad with \0 allowing round-tripping with
    /// [CdDrive::toc_as_raw_bytes][super::drive::CdDrive::toc_as_raw_bytes].
    ///
    /// # Panics
    /// Will panic if `bytes` are not of correct size or alignment for a [`CDROM_TOC`]
    pub fn from_raw_bytes(mut bytes: Vec<u8>) -> CDROM_TOC {
        // SAFETY: ffi spec has not changed
        assert_eq!(size_of::<CDROM_TOC>(), TOC_SIZE);

        match bytes.len() {
            TOC_SIZE => {
                // full dump. Either with padding or 100 tracks.
            }
            ..4 => panic!("insufficient bytes for a CDROM_TOC with 0 tracks"),
            4..TOC_SIZE => {
                // SAFETY: toc.Length is the first field: [u8;2] & indicates the length, in bytes,
                // of the table of contents data. This length value does not include the length of
                // the Length member itself (2 bytes).
                let len = u16::from_be_bytes(bytes[0..=1].try_into().unwrap())
                    .strict_cast::<usize>()
                    .strict_add(2);
                assert_eq!(bytes.len(), len);
            }
            _ => panic!("too many bytes for a CDROM_TOC"),
        };

        // SAFETY: bytes will fit in a CDROM_TOC
        assert!(bytes.len() <= TOC_SIZE);

        // Pad with null bytes to allow round-tripping
        bytes.resize(TOC_SIZE, 0);

        #[expect(unsafe_code, reason = "construction from raw bytes")]
        let toc: CDROM_TOC = unsafe {
            // SAFETY: correct size & alignment
            assert_eq!(size_of::<CDROM_TOC>(), bytes.len());
            assert_eq!(0, bytes.as_ptr().align_offset(align_of::<CDROM_TOC>()));

            *(bytes.as_ptr() as *const _)
        };

        let last_track = toc.LastTrack.strict_cast();
        for (i, track) in toc.TrackData.iter().enumerate() {
            let track_number = i + 1;
            if (1..=last_track).contains(&track_number) {
                assert_eq!(track_number, track.TrackNumber.strict_cast::<usize>())
            } else if track_number == last_track + 1 {
                // Leadout
                assert_eq!(0xaa, track.TrackNumber.strict_cast::<usize>())
            } else {
                assert_eq!(0, track.Reserved);
                assert_eq!(0, track.Reserved1);
                assert_eq!(0, track.TrackNumber);
                assert_eq!([0; 4], track.Address);
            }
        }
        toc
    }

    pub fn to_toc(self) -> io::Result<Toc> {
        let tocentries = &TocEntries::from(self);
        tocentries.try_into()
    }

    fn iter_audio(&self) -> impl Iterator<Item = TocEntry> {
        self.TrackData
            .iter()
            .filter(|track| (1..0xA9).contains(&track.TrackNumber) && track.control().is_audio())
            .map(TocEntry::from)
    }

    fn iter_data(&self) -> impl Iterator<Item = TocEntry> {
        self.TrackData
            .iter()
            .filter(|track| (1..0xA9).contains(&track.TrackNumber) && track.control().is_data())
            .map(TocEntry::from)
    }

    fn iter_leadouts(&self) -> impl Iterator<Item = TocEntry> {
        self.TrackData
            .iter()
            .filter(|track| track.TrackNumber == 170)
            .map(TocEntry::from)
    }
}

impl From<CDROM_TOC> for TocEntries {
    fn from(toc: CDROM_TOC) -> Self {
        let audio = toc.iter_audio().collect();
        let data = toc.iter_data().collect();
        let leadouts = toc.iter_leadouts().collect();
        TocEntries {
            audio,
            data,
            leadouts,
        }
    }
}

impl TRACK_DATA {
    pub fn control(&self) -> Control {
        #[expect(clippy::used_underscore_binding, reason = "ffi naming")]
        let control_code = self._bitfield;
        Control::from_bits_truncate(control_code)
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
        #[expect(clippy::used_underscore_binding, reason = "ffi naming")]
        let control = Control::from_bits_truncate(track_data._bitfield);
        Self {
            track,
            start,
            control,
        }
    }
}

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
            Frame::new(u32::from_le_bytes(data.next_chunk().unwrap()) as usize) + LEADIN;
        let duration_frames = Frame::new(u32::from_le_bytes(data.next_chunk().unwrap()) as usize);

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
    #[case(TheBoyWhoKnewTooMuch)]
    fn toc_basic_properties(#[case] album: TestAlbum) {
        let toc = album.load_cdrom_toc();
        for track in toc.TrackData.iter().take(toc.track_entries()) {
            dbg!(track.TrackNumber);
            dbg!(track.Reserved);
            dbg!(track.Reserved1);
            #[expect(clippy::used_underscore_binding, reason = "ffi field naming")]
            let bitfield = track._bitfield;
            eprintln!("bitfield: {bitfield:08b}");
            eprintln!("control: {}", track.control());
            dbg!(track.Address);
            dbg!(u32::from_be_bytes(track.Address));
            dbg!(Msf::from(Frame::new(
                u32::from_be_bytes(track.Address).strict_cast()
            )));
        }
        assert_eq!(toc.FirstTrack, album.expected_first_track());
        assert_eq!(
            toc.last_audio_track().unwrap(),
            album.expected_last_audio_track().strict_cast::<usize>()
        );
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn leadout(#[case] album: TestAlbum) {
        let toc = album.load_cdrom_toc();
        let leadout = toc.to_toc().unwrap().leadout();
        assert_eq!(
            leadout,
            album.expected_leadout().as_usize().strict_cast::<u32>()
        );
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn audio(#[case] album: TestAlbum) {
        let toc = album.load_cdrom_toc();
        let audio_tracks: Vec<TocEntry> = toc.iter_audio().collect();
        assert_eq!(audio_tracks, album.expected_audio_toc_entries());
    }

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

    #[test_log::test]
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn compare_toc(#[case] album: TestAlbum) {
        let cdrom_toc = album.load_cdrom_toc();
        let toc = album.expected_toc();

        assert_eq!(toc, cdrom_toc.to_toc().unwrap())
    }
}
