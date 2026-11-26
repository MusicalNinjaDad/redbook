//! Test fixtures for album data

use std::{collections::HashMap, fmt::Display, fs, io, path::PathBuf};

use metaflac::block::{Picture, PictureType};

use crate::{
    Frame, LEADIN, TocEntries,
    disc::Track,
    error::RedbookError,
    tags::artwork::picture_from_jpeg,
    test_fixtures::track_descriptor::RawTocDescriptors,
    toc::{ByteSize, Msf, TrackNo},
};

/// Test album identifier for parameterized tests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestAlbum {
    DefinitelyMaybe,
    TheWallDisc1,
    TheWallDisc2,
    BatOutOfHellRevamped,
    TheBoyWhoKnewTooMuch,
}

impl Display for TestAlbum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestAlbum::DefinitelyMaybe => write!(f, "DefinitelyMaybe"),
            TestAlbum::TheWallDisc1 => write!(f, "TheWallDisc1"),
            TestAlbum::TheWallDisc2 => write!(f, "TheWallDisc2"),
            TestAlbum::BatOutOfHellRevamped => write!(f, "BatOutOfHellRevamped"),
            TestAlbum::TheBoyWhoKnewTooMuch => write!(f, "TheBoyWhoKnewTooMuch"),
        }
    }
}

impl TryFrom<&PathBuf> for TestAlbum {
    type Error = io::Error;

    fn try_from(path: &PathBuf) -> io::Result<Self> {
        match path.absolute()? {
            p if p == TestAlbum::DefinitelyMaybe.assets_path().absolute()? => {
                Ok(TestAlbum::DefinitelyMaybe)
            }
            p if p == TestAlbum::TheWallDisc1.assets_path().absolute()? => {
                Ok(TestAlbum::TheWallDisc1)
            }
            p if p == TestAlbum::TheWallDisc2.assets_path().absolute()? => {
                Ok(TestAlbum::TheWallDisc2)
            }
            p if p == TestAlbum::BatOutOfHellRevamped.assets_path().absolute()? => {
                Ok(TestAlbum::BatOutOfHellRevamped)
            }
            p if p == TestAlbum::TheBoyWhoKnewTooMuch.assets_path().absolute()? => {
                Ok(TestAlbum::TheBoyWhoKnewTooMuch)
            }
            _ => Err(io::Error::new(io::ErrorKind::NotFound, "unknown album")),
        }
    }
}

impl TestAlbum {
    /// Path to the CDROM_TOC.hex file for this album
    pub fn cdrom_toc_path(&self) -> PathBuf {
        self.assets_path().join("CDROM_TOC.hex")
    }

    /// Path to the TOC.hex file for this album
    pub fn toc_path(&self) -> PathBuf {
        self.assets_path().join("TOC.hex")
    }

    /// Path to the assets directory for this album
    pub fn assets_path(&self) -> PathBuf {
        // To allow other crates to use these fixtures
        let redbook_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let assets = redbook_root.join("tests").join("assets");
        match self {
            TestAlbum::DefinitelyMaybe => assets.join("definitely_maybe"),
            TestAlbum::TheWallDisc1 => assets.join("the_wall").join("disc1"),
            TestAlbum::TheWallDisc2 => assets.join("the_wall").join("disc2"),
            TestAlbum::BatOutOfHellRevamped => assets.join("bat_out_of_hell_revamped"),
            TestAlbum::TheBoyWhoKnewTooMuch => assets.join("the_boy_who_knew_too_much"),
        }
    }

    /// Load and parse the TOC.hex file (READ TOC format 0b0010) as full [`TocEntries`]
    pub fn expected_toc_entries_b0010(&self) -> TocEntries {
        let toc_dump = super::load_hex_file(&self.toc_path());
        RawTocDescriptors::new(toc_dump)
            .and_then(TocEntries::try_from)
            .unwrap()
    }

    /// Load and parse the TOC.hex file
    pub fn expected_toc(&self) -> cdtoc::Toc {
        cdtoc::Toc::try_from(self.expected_toc_entries_b0010())
            .map_err(|error| error.into_io_error())
            .unwrap()
    }

    /// Expected first track number for this album
    pub fn expected_first_track(&self) -> u8 {
        match self {
            TestAlbum::DefinitelyMaybe => 1,
            TestAlbum::TheWallDisc1 => 1,
            TestAlbum::TheWallDisc2 => 1,
            TestAlbum::BatOutOfHellRevamped => 1,
            TestAlbum::TheBoyWhoKnewTooMuch => 1,
        }
    }

    /// Expected last track number for this album
    pub fn expected_last_audio_track(&self) -> u8 {
        match self {
            TestAlbum::DefinitelyMaybe => 11,
            TestAlbum::TheWallDisc1 => 13,
            TestAlbum::TheWallDisc2 => 13,
            TestAlbum::BatOutOfHellRevamped => 8,
            TestAlbum::TheBoyWhoKnewTooMuch => 12,
        }
    }

    /// Expected leadout frame for this album
    pub fn expected_leadout(&self) -> Frame {
        match self {
            TestAlbum::DefinitelyMaybe => Frame::from(Msf::new(0x34, 0x05, 0x1c)),
            TestAlbum::TheWallDisc1 => Frame::from(Msf::new(0x27, 0x0E, 0x0A)),
            TestAlbum::TheWallDisc2 => Frame::from(Msf::new(0x29, 0x3a, 0x19)),
            TestAlbum::BatOutOfHellRevamped => Frame::from(Msf::new(0x32, 0x39, 0x07)),
            TestAlbum::TheBoyWhoKnewTooMuch => Frame::strict_new(202248_u32).strict_add(LEADIN),
        }
    }

    /// Expected tracks as Track objects (without metadata)
    pub fn expected_audio_tracks_minimal(&self) -> Vec<Track<'static>> {
        let track_data: Vec<(u8, Msf, Frame)> = match self {
            TestAlbum::DefinitelyMaybe => vec![
                (1, Msf::new(0x00, 0x02, 0x21), Frame::strict_new(24242_u32)),
                (2, Msf::new(0x05, 0x19, 0x32), Frame::strict_new(23138_u32)),
                (3, Msf::new(0x0A, 0x22, 0x0D), Frame::strict_new(20762_u32)),
                (4, Msf::new(0x0F, 0x0B, 0x00), Frame::strict_new(20168_u32)),
                (5, Msf::new(0x13, 0x27, 0x44), Frame::strict_new(28272_u32)),
                (6, Msf::new(0x19, 0x38, 0x41), Frame::strict_new(21280_u32)),
                (7, Msf::new(0x1E, 0x28, 0x2D), Frame::strict_new(19338_u32)),
                (8, Msf::new(0x22, 0x3A, 0x21), Frame::strict_new(21700_u32)),
                (9, Msf::new(0x27, 0x2F, 0x3A), Frame::strict_new(11425_u32)),
                (10, Msf::new(0x2A, 0x14, 0x08), Frame::strict_new(29455_u32)),
                (11, Msf::new(0x30, 0x34, 0x3F), Frame::strict_new(14440_u32)),
            ],
            TestAlbum::TheWallDisc1 => vec![
                (1, Msf::new(0x00, 0x02, 0x00), Frame::strict_new(14967_u32)),
                (2, Msf::new(0x03, 0x15, 0x2A), Frame::strict_new(11240_u32)),
                (3, Msf::new(0x05, 0x33, 0x20), Frame::strict_new(14248_u32)),
                (4, Msf::new(0x09, 0x01, 0x1E), Frame::strict_new(8287_u32)),
                (5, Msf::new(0x0A, 0x33, 0x43), Frame::strict_new(17960_u32)),
                (6, Msf::new(0x0E, 0x33, 0x1B), Frame::strict_new(25040_u32)),
                (7, Msf::new(0x14, 0x19, 0x11), Frame::strict_new(12568_u32)),
                (8, Msf::new(0x17, 0x0C, 0x3C), Frame::strict_new(9625_u32)),
                (9, Msf::new(0x19, 0x15, 0x0A), Frame::strict_new(15832_u32)),
                (10, Msf::new(0x1C, 0x34, 0x11), Frame::strict_new(16240_u32)),
                (11, Msf::new(0x20, 0x1C, 0x39), Frame::strict_new(19230_u32)),
                (12, Msf::new(0x24, 0x2D, 0x0C), Frame::strict_new(5603_u32)),
                (13, Msf::new(0x25, 0x3B, 0x41), Frame::strict_new(5570_u32)),
            ],
            TestAlbum::TheWallDisc2 => vec![
                (1, Msf::new(0x00, 0x02, 0x00), Frame::strict_new(21115_u32)),
                (2, Msf::new(0x04, 0x2B, 0x28), Frame::strict_new(12020_u32)),
                (3, Msf::new(0x07, 0x17, 0x3C), Frame::strict_new(15340_u32)),
                (4, Msf::new(0x0A, 0x30, 0x19), Frame::strict_new(6980_u32)),
                (5, Msf::new(0x0C, 0x15, 0x1E), Frame::strict_new(6537_u32)),
                (6, Msf::new(0x0D, 0x30, 0x2A), Frame::strict_new(28628_u32)),
                (7, Msf::new(0x14, 0x0A, 0x14), Frame::strict_new(7210_u32)),
                (8, Msf::new(0x15, 0x2E, 0x1E), Frame::strict_new(19255_u32)),
                (9, Msf::new(0x1A, 0x03, 0x0A), Frame::strict_new(19780_u32)),
                (10, Msf::new(0x1E, 0x1A, 0x41), Frame::strict_new(17877_u32)),
                (11, Msf::new(0x22, 0x19, 0x11), Frame::strict_new(2268_u32)),
                (12, Msf::new(0x22, 0x37, 0x23), Frame::strict_new(23980_u32)),
                (13, Msf::new(0x28, 0x0F, 0x0F), Frame::strict_new(7735_u32)),
            ],
            TestAlbum::BatOutOfHellRevamped => vec![
                (
                    1,
                    Msf::new(0x00, 0x02, 0x00),
                    Frame::from(Msf::new(0x09, 0x34, 0x16)).strict_sub(Msf::new(0x00, 0x02, 0x00)),
                ),
                (
                    2,
                    Msf::new(0x09, 0x34, 0x16),
                    Frame::from(Msf::new(0x0e, 0x38, 0x34)).strict_sub(Msf::new(0x09, 0x34, 0x16)),
                ),
                (
                    3,
                    Msf::new(0x0e, 0x38, 0x34),
                    Frame::from(Msf::new(0x13, 0x25, 0x19)).strict_sub(Msf::new(0x0e, 0x38, 0x34)),
                ),
                (
                    4,
                    Msf::new(0x13, 0x25, 0x19),
                    Frame::from(Msf::new(0x17, 0x39, 0x00)).strict_sub(Msf::new(0x13, 0x25, 0x19)),
                ),
                (
                    5,
                    Msf::new(0x17, 0x39, 0x00),
                    Frame::from(Msf::new(0x1D, 0x16, 0x0A)).strict_sub(Msf::new(0x17, 0x39, 0x00)),
                ),
                (
                    6,
                    Msf::new(0x1D, 0x16, 0x0A),
                    Frame::from(Msf::new(0x25, 0x32, 0x05)).strict_sub(Msf::new(0x1D, 0x16, 0x0A)),
                ),
                (
                    7,
                    Msf::new(0x25, 0x32, 0x05),
                    Frame::from(Msf::new(0x2e, 0x21, 0x19)).strict_sub(Msf::new(0x25, 0x32, 0x05)),
                ),
                (
                    8,
                    Msf::new(0x2e, 0x21, 0x19),
                    Frame::from(Msf::new(0x32, 0x39, 0x07)).strict_sub(Msf::new(0x2e, 0x21, 0x19)),
                ),
            ],
            TestAlbum::TheBoyWhoKnewTooMuch => vec![
                (
                    0x01,
                    Msf::new(0x00, 0x02, 0x00),
                    Frame::from(Msf::new(0x04, 0x00, 0x07)).strict_sub(Msf::new(0x00, 0x02, 0x00)),
                ),
                (
                    0x02,
                    Msf::new(0x04, 0x00, 0x07),
                    Frame::from(Msf::new(0x07, 0x22, 0x2B)).strict_sub(Msf::new(0x04, 0x00, 0x07)),
                ),
                (
                    0x03,
                    Msf::new(0x07, 0x22, 0x2B),
                    Frame::from(Msf::new(0x0B, 0x12, 0x18)).strict_sub(Msf::new(0x07, 0x22, 0x2B)),
                ),
                (
                    0x04,
                    Msf::new(0x0B, 0x12, 0x18),
                    Frame::from(Msf::new(0x0F, 0x02, 0x1E)).strict_sub(Msf::new(0x0B, 0x12, 0x18)),
                ),
                (
                    0x05,
                    Msf::new(0x0F, 0x02, 0x1E),
                    Frame::from(Msf::new(0x13, 0x12, 0x44)).strict_sub(Msf::new(0x0F, 0x02, 0x1E)),
                ),
                (
                    0x06,
                    Msf::new(0x13, 0x12, 0x44),
                    Frame::from(Msf::new(0x16, 0x09, 0x0D)).strict_sub(Msf::new(0x13, 0x12, 0x44)),
                ),
                (
                    0x07,
                    Msf::new(0x16, 0x09, 0x0D),
                    Frame::from(Msf::new(0x19, 0x0B, 0x07)).strict_sub(Msf::new(0x16, 0x09, 0x0D)),
                ),
                (
                    0x08,
                    Msf::new(0x19, 0x0B, 0x07),
                    Frame::from(Msf::new(0x1C, 0x1F, 0x06)).strict_sub(Msf::new(0x19, 0x0B, 0x07)),
                ),
                (
                    0x09,
                    Msf::new(0x1C, 0x1F, 0x06),
                    Frame::from(Msf::new(0x1F, 0x34, 0x15)).strict_sub(Msf::new(0x1C, 0x1F, 0x06)),
                ),
                (
                    0x0A,
                    Msf::new(0x1F, 0x34, 0x15),
                    Frame::from(Msf::new(0x22, 0x33, 0x02)).strict_sub(Msf::new(0x1F, 0x34, 0x15)),
                ),
                (
                    0x0B,
                    Msf::new(0x22, 0x33, 0x02),
                    Frame::from(Msf::new(0x25, 0x31, 0x05)).strict_sub(Msf::new(0x22, 0x33, 0x02)),
                ),
                (
                    0x0C,
                    Msf::new(0x25, 0x31, 0x05),
                    Frame::strict_new(15221_u32),
                ),
            ],
        };
        track_data
            .into_iter()
            .map(|(trackno, start, duration)| {
                Track::new(TrackNo::strict_new(trackno), Frame::from(start), duration)
            })
            .collect()
    }

    /// Load musicbrainz data from the musicbrainz.json file for this album
    pub fn expected_musicbrainz(&self) -> musicbrainz_rs::entity::discid::Discid {
        let path = self.assets_path().join("musicbrainz.json");
        let json_content = std::fs::read_to_string(&path)
            .unwrap_or_else(|_| panic!("Failed to read musicbrainz.json from {:?}", path));
        serde_json::from_str(&json_content)
            .unwrap_or_else(|e| panic!("Failed to parse musicbrainz.json from {:?}: {}", path, e))
    }

    /// Indices of releases, sorted by title then newest-oldest
    pub fn expected_release_order(&self) -> Vec<usize> {
        match self {
            TestAlbum::DefinitelyMaybe => vec![4, 5, 1, 2, 0, 3],
            TestAlbum::TheWallDisc1 => vec![1, 3, 0, 7, 6, 5, 2, 4],
            TestAlbum::TheWallDisc2 => vec![1, 3, 0, 7, 6, 5, 2, 4],
            TestAlbum::BatOutOfHellRevamped => vec![0],
            TestAlbum::TheBoyWhoKnewTooMuch => vec![3],
        }
    }

    /// The releases sorted by title then newest-oldest
    pub fn expected_releases_in_order(&self) -> Vec<musicbrainz_rs::entity::release::Release> {
        let order = self.expected_release_order();
        let releases = self.expected_musicbrainz().releases.unwrap();
        order.iter().map(|&i| releases[i].clone()).collect()
    }

    /// Load the expected release menu from release_selection.txt
    pub fn expected_release_menu(&self) -> Option<String> {
        let path = self.assets_path().join("release_selection.txt");
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if content.trim().is_empty() {
                    None
                } else {
                    Some(content)
                }
            }
            Err(_) => None,
        }
    }

    /// Path to the thumbnail
    pub fn thumbnail_path(&self) -> PathBuf {
        self.assets_path().join("thumbnail.jpeg")
    }

    /// The cover art as a [`Picture`], read from the album's thumbnail
    pub fn expected_coverart(&self) -> Picture {
        let thumb = fs::read(self.thumbnail_path()).unwrap();
        picture_from_jpeg(PictureType::CoverFront, "Front Cover", thumb)
    }

    /// The cover thumbnails
    pub fn expected_thumbnails(&self) -> HashMap<String, Picture> {
        fs::read_dir(self.assets_path().join("thumbs"))
            .unwrap()
            .map(|file| {
                let pic = file.unwrap().path();
                let thumb = fs::read(&pic).unwrap();
                let image = picture_from_jpeg(PictureType::CoverFront, "Front Cover", thumb);
                (
                    pic.file_stem().unwrap().to_string_lossy().into_owned(),
                    image,
                )
            })
            .collect()
    }

    /// The correct release_id for the album
    pub fn release_id(&self) -> Option<&str> {
        match self {
            TestAlbum::DefinitelyMaybe => Some("9822581d-98bf-3f97-a94c-4b1350d090aa"),
            TestAlbum::TheWallDisc1 => Some("b13b64f6-85fc-3c1c-8aae-e5adb94d7181"),
            TestAlbum::TheWallDisc2 => Some("b13b64f6-85fc-3c1c-8aae-e5adb94d7181"),
            TestAlbum::BatOutOfHellRevamped => Some("ffe90874-5010-454b-9877-6c974f602d96"),
            TestAlbum::TheBoyWhoKnewTooMuch => Some("a06f94a2-0317-4fe7-b56c-18df3f5ac012"),
        }
    }

    /// The correct release number for the album
    pub fn release(&self) -> usize {
        match self {
            TestAlbum::DefinitelyMaybe => 2,
            TestAlbum::TheWallDisc1 => 2,
            TestAlbum::TheWallDisc2 => 2,
            TestAlbum::BatOutOfHellRevamped => 0,
            TestAlbum::TheBoyWhoKnewTooMuch => 3,
        }
    }

    /// The disc_index as it should be automatically identified.
    pub fn expected_disc_index(&self) -> Option<usize> {
        match self {
            TestAlbum::DefinitelyMaybe => Some(0),
            TestAlbum::TheWallDisc1 => self
                .expected_musicbrainz()
                .releases
                .unwrap()
                .get(self.release())
                .map(|release| {
                    release
                        .media
                        .as_ref()
                        .unwrap()
                        .iter()
                        .position(|media| media.position == Some(1))
                })
                .unwrap(),
            TestAlbum::TheWallDisc2 => self
                .expected_musicbrainz()
                .releases
                .unwrap()
                .get(self.release())
                .map(|release| {
                    release
                        .media
                        .as_ref()
                        .unwrap()
                        .iter()
                        .position(|media| media.position == Some(2))
                })
                .unwrap(),
            TestAlbum::BatOutOfHellRevamped => Some(0),
            TestAlbum::TheBoyWhoKnewTooMuch => Some(0),
        }
    }

    /// The fake OS-provided value for max_chunk() on any device
    ///
    /// Note this is deliberately *NOT* always a valid `ByteSize` to represent more realistic
    /// responses
    pub fn max_chunk(&self) -> usize {
        const KB: usize = 1024;
        match self {
            TestAlbum::DefinitelyMaybe => 64_usize.strict_mul(KB), // win USB, aligned
            TestAlbum::TheWallDisc1 => Frame::strict_new(75).size().strict_cast(), // linux
            TestAlbum::TheWallDisc2 => 63_usize.strict_mul(KB),    // win USB, unaligned
            TestAlbum::BatOutOfHellRevamped => 128_usize.strict_mul(KB), // win ATAPI/SCSI
            TestAlbum::TheBoyWhoKnewTooMuch => 102400_usize.strict_mul(KB), // huge: 100MB,
        }
    }

    /// The correct fake value for max_chunk() on any device
    ///
    /// Note this is deliberately *NOT* always a valid `ByteSize` to represent more realistic
    /// responses
    pub fn expected_max_chunk(&self) -> ByteSize {
        const KB: usize = 1024;
        let raw = match self {
            TestAlbum::DefinitelyMaybe => 64_usize.strict_mul(KB), // win USB, aligned
            TestAlbum::TheWallDisc1 => Frame::strict_new(75).size().strict_cast(), // linux
            TestAlbum::TheWallDisc2 => 63_usize.strict_mul(KB),    // win USB, unaligned
            TestAlbum::BatOutOfHellRevamped => 128_usize.strict_mul(KB), // win ATAPI/SCSI
            TestAlbum::TheBoyWhoKnewTooMuch => 102400_usize.strict_mul(KB), // huge: 100MB,
        };
        ByteSize::rounded_new(raw)
    }
}
