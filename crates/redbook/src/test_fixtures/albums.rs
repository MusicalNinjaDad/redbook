#![expect(missing_docs, reason = "needs update")]
//! Test fixtures for album data

use std::{collections::HashMap, fmt::Display, fs, io, path::PathBuf};

use metaflac::block::{Picture, PictureType};

use crate::{
    Frame, Msf, TocEntry, Track, tagging::PictureExt, toc::TocString, win::toc::CDROM_TOC,
};

/// Test album identifier for parameterized tests
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestAlbum {
    DefinitelyMaybe,
    TheWallDisc1,
    TheWallDisc2,
}

impl Display for TestAlbum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestAlbum::DefinitelyMaybe => write!(f, "DefinitelyMaybe"),
            TestAlbum::TheWallDisc1 => write!(f, "TheWallDisc1"),
            TestAlbum::TheWallDisc2 => write!(f, "TheWallDisc2"),
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
        }
    }

    /// Load and parse the TOC.hex file
    pub fn expected_toc(&self) -> cdtoc::Toc {
        let path = self.toc_path();
        let toc_dump = super::load_hex_file(&path);
        let toc_string = TocString::from_scsi_readtoc_0010b(toc_dump).unwrap();
        cdtoc::Toc::from(toc_string)
    }

    /// Load and parse the CDROM_TOC.hex file
    pub fn load_cdrom_toc(&self) -> CDROM_TOC {
        let path = self.cdrom_toc_path();
        let toc_dump = super::load_hex_file(&path);
        CDROM_TOC::from_raw_bytes(toc_dump)
    }

    /// Expected first track number for this album
    pub fn expected_first_track(&self) -> u8 {
        match self {
            TestAlbum::DefinitelyMaybe => 1,
            TestAlbum::TheWallDisc1 => 1,
            TestAlbum::TheWallDisc2 => 1,
        }
    }

    /// Expected last track number for this album
    pub fn expected_last_track(&self) -> u8 {
        match self {
            TestAlbum::DefinitelyMaybe => 11,
            TestAlbum::TheWallDisc1 => 13,
            TestAlbum::TheWallDisc2 => 13,
        }
    }

    /// Expected leadout frame for this album
    pub fn expected_leadout(&self) -> Frame {
        match self {
            TestAlbum::DefinitelyMaybe => Frame::from(Msf::new(0x34, 0x05, 0x1c)),
            TestAlbum::TheWallDisc1 => Frame::from(Msf::new(0x27, 0x0E, 0x0A)),
            TestAlbum::TheWallDisc2 => Frame::from(Msf::new(0x29, 0x3a, 0x19)),
        }
    }

    /// Get just the TocEntry values for comparison with iter_audio()
    pub fn expected_toc_entries(&self) -> Vec<TocEntry> {
        self.expected_tracks_minimal()
            .into_iter()
            .map(|track| track.toc_entry)
            .collect()
    }

    /// Expected tracks as Track objects (without metadata)
    pub fn expected_tracks_minimal(&self) -> Vec<Track<'static>> {
        match self {
            TestAlbum::DefinitelyMaybe => vec![
                Track {
                    toc_entry: TocEntry {
                        track: 1,
                        start: Frame::from(Msf::new(0x00, 0x02, 0x21)),
                    },
                    duration: Frame::new(24242),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 2,
                        start: Frame::from(Msf::new(0x05, 0x19, 0x32)),
                    },
                    duration: Frame::new(23138),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 3,
                        start: Frame::from(Msf::new(0x0A, 0x22, 0x0D)),
                    },
                    duration: Frame::new(20762),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 4,
                        start: Frame::from(Msf::new(0x0F, 0x0B, 0x00)),
                    },
                    duration: Frame::new(20168),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 5,
                        start: Frame::from(Msf::new(0x13, 0x27, 0x44)),
                    },
                    duration: Frame::new(28272),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 6,
                        start: Frame::from(Msf::new(0x19, 0x38, 0x41)),
                    },
                    duration: Frame::new(21280),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 7,
                        start: Frame::from(Msf::new(0x1E, 0x28, 0x2D)),
                    },
                    duration: Frame::new(19338),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 8,
                        start: Frame::from(Msf::new(0x22, 0x3A, 0x21)),
                    },
                    duration: Frame::new(21700),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 9,
                        start: Frame::from(Msf::new(0x27, 0x2F, 0x3A)),
                    },
                    duration: Frame::new(11425),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 10,
                        start: Frame::from(Msf::new(0x2A, 0x14, 0x08)),
                    },
                    duration: Frame::new(29455),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 11,
                        start: Frame::from(Msf::new(0x30, 0x34, 0x3F)),
                    },
                    duration: Frame::new(14440),
                    ..Default::default()
                },
            ],
            TestAlbum::TheWallDisc1 => vec![
                Track {
                    toc_entry: TocEntry {
                        track: 1,
                        start: Frame::from(Msf::new(0x00, 0x02, 0x00)),
                    },
                    duration: Frame::new(14967),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 2,
                        start: Frame::from(Msf::new(0x03, 0x15, 0x2A)),
                    },
                    duration: Frame::new(11240),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 3,
                        start: Frame::from(Msf::new(0x05, 0x33, 0x20)),
                    },
                    duration: Frame::new(14248),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 4,
                        start: Frame::from(Msf::new(0x09, 0x01, 0x1E)),
                    },
                    duration: Frame::new(8287),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 5,
                        start: Frame::from(Msf::new(0x0A, 0x33, 0x43)),
                    },
                    duration: Frame::new(17960),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 6,
                        start: Frame::from(Msf::new(0x0E, 0x33, 0x1B)),
                    },
                    duration: Frame::new(25040),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 7,
                        start: Frame::from(Msf::new(0x14, 0x19, 0x11)),
                    },
                    duration: Frame::new(12568),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 8,
                        start: Frame::from(Msf::new(0x17, 0x0C, 0x3C)),
                    },
                    duration: Frame::new(9625),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 9,
                        start: Frame::from(Msf::new(0x19, 0x15, 0x0A)),
                    },
                    duration: Frame::new(15832),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 10,
                        start: Frame::from(Msf::new(0x1C, 0x34, 0x11)),
                    },
                    duration: Frame::new(16240),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 11,
                        start: Frame::from(Msf::new(0x20, 0x1C, 0x39)),
                    },
                    duration: Frame::new(19230),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 12,
                        start: Frame::from(Msf::new(0x24, 0x2D, 0x0C)),
                    },
                    duration: Frame::new(5603),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 13,
                        start: Frame::from(Msf::new(0x25, 0x3B, 0x41)),
                    },
                    duration: Frame::new(5570),
                    ..Default::default()
                },
            ],
            TestAlbum::TheWallDisc2 => vec![
                Track {
                    toc_entry: TocEntry {
                        track: 1,
                        start: Frame::from(Msf::new(0x00, 0x02, 0x00)),
                    },
                    duration: Frame::new(21115),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 2,
                        start: Frame::from(Msf::new(0x04, 0x2B, 0x28)),
                    },
                    duration: Frame::new(12020),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 3,
                        start: Frame::from(Msf::new(0x07, 0x17, 0x3C)),
                    },
                    duration: Frame::new(15340),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 4,
                        start: Frame::from(Msf::new(0x0A, 0x30, 0x19)),
                    },
                    duration: Frame::new(6980),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 5,
                        start: Frame::from(Msf::new(0x0C, 0x15, 0x1E)),
                    },
                    duration: Frame::new(6537),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 6,
                        start: Frame::from(Msf::new(0x0D, 0x30, 0x2A)),
                    },
                    duration: Frame::new(28628),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 7,
                        start: Frame::from(Msf::new(0x14, 0x0A, 0x14)),
                    },
                    duration: Frame::new(7210),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 8,
                        start: Frame::from(Msf::new(0x15, 0x2E, 0x1E)),
                    },
                    duration: Frame::new(19255),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 9,
                        start: Frame::from(Msf::new(0x1A, 0x03, 0x0A)),
                    },
                    duration: Frame::new(19780),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 10,
                        start: Frame::from(Msf::new(0x1E, 0x1A, 0x41)),
                    },
                    duration: Frame::new(17877),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 11,
                        start: Frame::from(Msf::new(0x22, 0x19, 0x11)),
                    },
                    duration: Frame::new(2268),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 12,
                        start: Frame::from(Msf::new(0x22, 0x37, 0x23)),
                    },
                    duration: Frame::new(23980),
                    ..Default::default()
                },
                Track {
                    toc_entry: TocEntry {
                        track: 13,
                        start: Frame::from(Msf::new(0x28, 0x0F, 0x0F)),
                    },
                    duration: Frame::new(7735),
                    ..Default::default()
                },
            ],
        }
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

    /// The cover thumbnails
    pub fn expected_thumbnails(&self) -> HashMap<String, Picture> {
        fs::read_dir(self.assets_path().join("thumbs"))
            .unwrap()
            .map(|file| {
                let pic = file.unwrap().path();
                let thumb = fs::read(&pic).unwrap();
                let image = Picture::from_jpeg(PictureType::CoverFront, "Front Cover", thumb);
                (
                    pic.file_stem().unwrap().to_string_lossy().into_owned(),
                    image,
                )
            })
            .collect()
    }

    /// The correct release number for the album
    pub fn release(&self) -> usize {
        match self {
            TestAlbum::DefinitelyMaybe => 2,
            TestAlbum::TheWallDisc1 => self
                .expected_musicbrainz()
                .releases
                .unwrap()
                .iter()
                .position(|release| release.id == "b13b64f6-85fc-3c1c-8aae-e5adb94d7181")
                .unwrap(),
            TestAlbum::TheWallDisc2 => self
                .expected_musicbrainz()
                .releases
                .unwrap()
                .iter()
                .position(|release| release.id == "b13b64f6-85fc-3c1c-8aae-e5adb94d7181")
                .unwrap(),
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
        }
    }
}
