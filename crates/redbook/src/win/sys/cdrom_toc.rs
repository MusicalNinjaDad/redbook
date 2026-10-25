use std::io;

use crate::{
    Frame, LEADIN, TocEntries,
    test_fixtures::{albums::TestAlbum, load_hex_file},
    toc::Control,
    toc::TocEntry,
};

use super::{CDROM_TOC, TRACK_DATA};

/// size of ffi struct [`CDROM_TOC`]. TODO: think this is pointless as currently implemented.
const TOC_SIZE: usize = size_of::<CDROM_TOC>();

impl CDROM_TOC {
    /// Returns the full size, in bytes of the data-carrying part of the struct.
    ///
    /// While `Self.Length` indicates the length, in bytes, of the table of contents data, it does
    /// not include the length of the Length member itself. This function returns the total size,
    /// including the Length member.
    pub(super) fn size(&self) -> usize {
        u16::from_be_bytes(self.Length)
            .strict_cast::<usize>()
            .strict_add(size_of_val(&self.Length))
    }

    /// Returns the number of actual entries in the TrackData array. This is calculated based
    /// upon the total size of the struct.
    fn track_entries(&self) -> impl Iterator<Item = &TRACK_DATA> {
        let entries_size = self
            .size()
            .strict_sub(size_of_val(&self.Length))
            .strict_sub(size_of_val(&self.FirstTrack))
            .strict_sub(size_of_val(&self.LastTrack));
        entries_size
            .div_exact(size_of::<TRACK_DATA>().strict_cast())
            .expect(
                "size used by `TrackDetails` must be exactly divisible by `size_of::<TRACK_DATA>`",
            );
        self.TrackData.iter().take(entries_size)
    }

    /// Parse raw bytes as a [`CDROM_TOC`] structure. This expects the relevant bytes
    /// and will pad with \0 allowing round-tripping with
    /// [CdDrive::toc_as_raw_bytes][super::drive::CdDrive::toc_as_raw_bytes].
    ///
    /// # Panics
    /// Will panic if `bytes` are not of correct size or alignment for a [`CDROM_TOC`]
    fn from_raw_bytes(mut bytes: Vec<u8>) -> CDROM_TOC {
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

    pub fn as_raw_bytes(&self) -> Vec<u8> {
        // SAFETY check: stored value is the expected size, nothing has changed in the ffi spec
        // TODO: should use size_of_val here to avoid assert!(true)
        const { assert!(size_of::<CDROM_TOC>() == TOC_SIZE) };

        // SAFETY: toc.Length Indicates the length, in bytes, of the table of contents data.
        // This length value does not include the length of the Length member itself.
        let len = self.size();
        assert!(len <= TOC_SIZE);

        let bytes: &[u8] = unsafe {
            // SAFETY:
            // - correct size validated above, we are not reading past the end of valid data
            // - CDROM_TOC is Copy, slice points to current stack. Slice is therefore only valid
            //   for duration of current function.
            // - Return type is owned copy of this data as a Vec<u8>.
            std::slice::from_raw_parts(self as *const _ as *const _, len)
        };

        Vec::from(bytes)
    }

    fn iter_audio(&self) -> impl Iterator<Item = TocEntry> {
        self.track_entries()
            .filter(|track| (1..0xA9).contains(&track.TrackNumber) && track.control().is_audio())
            .map(TocEntry::from)
    }

    fn iter_data(&self) -> impl Iterator<Item = TocEntry> {
        self.track_entries()
            .filter(|track| (1..0xA9).contains(&track.TrackNumber) && track.control().is_data())
            .map(TocEntry::from)
    }

    fn iter_leadouts(&self) -> impl Iterator<Item = TocEntry> {
        self.track_entries()
            .filter(|track| track.TrackNumber == 170)
            .map(TocEntry::from)
    }
}

impl From<TestAlbum> for CDROM_TOC {
    fn from(album: TestAlbum) -> Self {
        let path = album.cdrom_toc_path();
        let toc_dump = load_hex_file(&path);
        Self::from_raw_bytes(toc_dump)
    }
}

impl TryFrom<CDROM_TOC> for TocEntries {
    type Error = io::Error;

    fn try_from(toc: CDROM_TOC) -> Result<Self, Self::Error> {
        let audio = toc.iter_audio();
        let data = toc.iter_data();
        let leadouts = toc.iter_leadouts();
        audio.chain(data).chain(leadouts).collect()
    }
}

impl TRACK_DATA {
    fn control(&self) -> Control {
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

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {
    use super::*;

    use std::io;

    use cdtoc::Toc;
    use rstest::rstest;

    use crate::{
        test_fixtures::albums::TestAlbum::{self, *},
        toc::TocEntryFilter,
    };

    impl CDROM_TOC {
        fn to_toc(self) -> io::Result<Toc> {
            let tocentries = &TocEntries::try_from(self)?;
            tocentries.try_into()
        }

        /// Only takes the claimed number of tracks, thus avoiding the leadouts.
        ///
        /// TODO: TRACK_DATA::is_leadout()
        fn last_audio_track(&self) -> Option<usize> {
            self.TrackData
                .iter()
                .take(self.LastTrack.strict_cast())
                .rfind(|track| !track.control().contains(Control::DATA_TRACK))
                .map(|track| track.TrackNumber.strict_cast())
        }
    }

    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn toc_basic_properties(#[case] album: TestAlbum) {
        let toc = CDROM_TOC::from(album);
        for track in toc.track_entries() {
            use crate::frame::Msf;

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
        let toc = CDROM_TOC::from(album);
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
        let toc = CDROM_TOC::from(album);
        let audio_tracks: Vec<TocEntry> = toc.iter_audio().collect();
        assert_eq!(audio_tracks, album.expected_audio_toc_entries());
    }

    #[test_log::test]
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn compare_toc(#[case] album: TestAlbum) {
        let cdrom_toc = CDROM_TOC::from(album);
        let toc = album.expected_toc();

        assert_eq!(toc, cdrom_toc.to_toc().unwrap())
    }

    #[test_log::test]
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn toc_entries_from_cdrom_toc(#[case] album: TestAlbum) {
        let wintoc = CDROM_TOC::from(album);
        let entries = TocEntries::try_from(wintoc).unwrap();
        assert_eq!(
            entries.iter().audio().cloned().collect::<Vec<_>>(),
            album.expected_audio_toc_entries()
        );
        assert_eq!(
            entries.audio_tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
    }
}
