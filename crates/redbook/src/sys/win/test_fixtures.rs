//! Test fixtures bridging [`TestAlbum`] assets and the Windows `CDROM_TOC` ffi struct.
//!
//! The `CDROM_TOC.hex` assets are dumps obtained via the Windows API
//! (`IOCTL_CDROM_READ_TOC_EX`, READ TOC format 0b0000), in contrast to the 0b0010 dumps in
//! `TOC.hex` (see [`track_descriptor`][crate::test_fixtures::track_descriptor]).

use crate::{
    TocEntries,
    test_fixtures::{albums::TestAlbum, load_hex_file},
};

use super::bindings::CDROM_TOC;

impl From<TestAlbum> for CDROM_TOC {
    fn from(album: TestAlbum) -> Self {
        let path = album.cdrom_toc_path();
        let toc_dump = load_hex_file(&path);
        Self::from_raw_bytes(toc_dump)
    }
}

impl TestAlbum {
    /// Load and parse the CDROM_TOC.hex asset (Windows `CDROM_TOC`, READ TOC format 0b0000) as
    /// full [`TocEntries`]
    pub fn expected_win_toc(&self) -> TocEntries {
        TocEntries::try_from(CDROM_TOC::from(*self)).unwrap()
    }
}

#[cfg(test)]
#[forbid(unsafe_code)]
mod tests {
    use super::*;

    use cdtoc::Toc;
    use rstest::rstest;
    use tracing_test::traced_test;

    use crate::{
        Device,
        sys::win::DriveHandle,
        test_fixtures::albums::TestAlbum::{self, *},
        toc::TocError,
    };

    impl CDROM_TOC {
        fn to_toc(self) -> Result<Toc, TocError> {
            let tocentries = &TocEntries::try_from(self)?;
            tocentries.try_into()
        }
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
        assert_eq!(leadout, album.expected_leadout().cast::<u32>());
    }

    #[traced_test] // TODO: #181 use tracing_test to assert output
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn compare_toc(#[case] album: TestAlbum) {
        let cdrom_toc = CDROM_TOC::from(album);
        let toc = album.expected_toc();

        assert_eq!(toc, cdrom_toc.to_toc().unwrap());
    }

    #[traced_test] // TODO: #181 use tracing_test to assert output
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn toc_entries_from_cdrom_toc(#[case] album: TestAlbum) {
        let wintoc = CDROM_TOC::from(album);
        let entries = TocEntries::try_from(wintoc).unwrap();
        assert_eq!(entries, album.expected_win_toc());
        assert_eq!(
            entries.audio_tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
    }

    /// The TOC loaded from the (mock) disc via the [`Device`] API and the parsed
    /// [`TestAlbum::expected_win_toc`] must both agree with the manually defined
    /// [`TestAlbum::expected_audio_tracks_minimal`].
    #[traced_test] // TODO: #181 use tracing_test to assert output
    #[rstest]
    #[case(DefinitelyMaybe)]
    #[case(TheWallDisc1)]
    #[case(TheWallDisc2)]
    #[case(BatOutOfHellRevamped)]
    #[case(TheBoyWhoKnewTooMuch)]
    fn loaded_from_disc_matches_expected_audio_tracks(#[case] album: TestAlbum) {
        let drive = DriveHandle::open(&album.assets_path()).unwrap();
        let from_disc = drive.read_toc().unwrap();
        let expected = album.expected_win_toc();

        assert_eq!(from_disc, expected);
        assert_eq!(
            from_disc.audio_tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
        assert_eq!(
            expected.audio_tracks().collect::<Vec<_>>(),
            album.expected_audio_tracks_minimal()
        );
    }
}
