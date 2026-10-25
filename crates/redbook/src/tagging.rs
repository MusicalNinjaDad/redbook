#![expect(missing_docs, reason = "needs update")]
//! Tagging utilities for FLAC metadata

use std::{
    fmt::{Debug, Display},
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    str::FromStr,
};

use metaflac::{
    Tag,
    block::{Picture, PictureType, VorbisComment},
};
use musicbrainz_rs::entity::{
    artist_credit::ArtistCredit,
    release::{Release, ReleaseStatus, Track},
    release_scripts::ReleaseScript,
};
use tracing_result::Trace;
use zune_jpeg::{JpegDecoder, zune_core::bytestream::ZCursor};

pub trait ArtistCreditsExt {
    fn artist_names(&self) -> impl Iterator<Item = String>;
    fn artist_ids(&self) -> impl Iterator<Item = String>;
    fn main_artist(&self) -> Option<String> {
        self.artist_names().nth(0)
    }
}

impl ArtistCreditsExt for Release {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_names()
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_ids()
    }
}

impl ArtistCreditsExt for Track {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_names()
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.artist_credit.artist_ids()
    }
}

impl ArtistCreditsExt for Vec<ArtistCredit> {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.name.clone())
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.artist.id.clone())
    }
}

impl ArtistCreditsExt for &Vec<ArtistCredit> {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.name.clone())
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.iter().map(|credit| credit.artist.id.clone())
    }
}

impl<T: ArtistCreditsExt> ArtistCreditsExt for Option<T> {
    fn artist_names(&self) -> impl Iterator<Item = String> {
        self.as_ref()
            .map(|credits| credits.artist_names().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }

    fn artist_ids(&self) -> impl Iterator<Item = String> {
        self.as_ref()
            .map(|credits| credits.artist_ids().collect::<Vec<_>>())
            .unwrap_or_default()
            .into_iter()
    }
}

pub trait PictureExt {
    fn from_jpeg<B: AsRef<[u8]>, S: ToString>(
        picture_type: PictureType,
        description: S,
        data: B,
    ) -> Self;

    fn filename(&self) -> io::Result<PathBuf>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum ImageMimeTypes {
    Jpeg,
}

impl Display for ImageMimeTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "image/")?;
        match self {
            ImageMimeTypes::Jpeg => write!(f, "jpeg"),
        }
    }
}

impl FromStr for ImageMimeTypes {
    type Err = io::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mimetype = match s {
            "image/jpeg" => ImageMimeTypes::Jpeg,
            _ => Err(io::Error::new(ErrorKind::InvalidData, "unknown mime type"))
                .or_warn("parsing artwork mime type")?,
        };
        Ok(mimetype)
    }
}

impl PictureExt for Picture {
    fn from_jpeg<B: AsRef<[u8]>, S: ToString>(
        picture_type: PictureType,
        description: S,
        data: B,
    ) -> Self {
        let mut jpg_info = JpegDecoder::new(ZCursor::from(&data));

        #[expect(unused_must_use, reason = "TODO: change to try_from_jpeg")]
        jpg_info.decode_headers();

        let width = jpg_info.info().unwrap().width as u32;
        let height = jpg_info.info().unwrap().height as u32;

        Picture {
            picture_type,
            mime_type: ImageMimeTypes::Jpeg.to_string(),
            description: description.to_string(),
            width,
            height,
            depth: 24,
            num_colors: 0,
            data: data.as_ref().to_vec(),
        }
    }

    fn filename(&self) -> io::Result<PathBuf> {
        let _debug_span = tracing::debug_span!("PictureExt::filename").entered();
        let filename = match self.picture_type {
            PictureType::CoverFront => "cover",
            _ => todo!("TODO #100: handle other picture types"),
        };
        let extension = match self
            .mime_type
            .parse::<ImageMimeTypes>()
            .or_warn("identifying image mime type")?
        {
            ImageMimeTypes::Jpeg => "jpg",
        };
        let path = PathBuf::from(sanitise(filename)).with_extension(sanitise(extension));
        tracing::debug!(path = %path.display());
        Ok(path)
    }
}

pub trait VorbisTagExt {
    /// 0n Full - title
    fn filename(&self) -> PathBuf;

    /// Album - Artists/Album title [Disc n]
    fn directory(&self) -> PathBuf;

    /// Title one - Title two
    fn full_title(&self) -> String;

    /// Write tags to the given file
    fn add_to<P: AsRef<Path>>(self, path: &P) -> io::Result<()>;

    fn discno(&self) -> Option<String>;

    fn total_discs(&self) -> Option<String>;

    fn set_discno(&mut self, discno: usize);

    fn set_total_discs(&mut self, discs: usize);

    /// Album 1 - Album 2 [Disc 2]
    fn full_album(&self) -> Option<String>;
}

impl VorbisTagExt for VorbisComment {
    fn filename(&self) -> PathBuf {
        let track_number = format!("{:02}", self.track().unwrap_or_default());
        let title = self.full_title();
        PathBuf::from(sanitise(&[track_number, title].join(" ")))
    }

    fn directory(&self) -> PathBuf {
        let artist = self
            .album_artist()
            .map(|artists| artists.join(" "))
            .unwrap_or_else(|| "Unknown artist".to_string());
        let title = self
            .full_album()
            .unwrap_or_else(|| "Unknown album".to_string());
        PathBuf::from(sanitise(&artist)).join(sanitise(&title))
    }

    fn full_title(&self) -> String {
        self.title()
            .map(|titles| titles.join(" - "))
            .unwrap_or_default()
    }

    fn add_to<P: AsRef<Path>>(self, path: &P) -> io::Result<()> {
        let mut file_tag = Tag::read_from_path(path.as_ref())
            .map_err(|error| io::Error::new(ErrorKind::InvalidData, error))
            .or_warn("reading tags from file")?;
        file_tag
            .vorbis_comments_mut()
            .comments
            .extend(self.comments);
        file_tag
            .write_to_path(path.as_ref())
            .map_err(|error| match error.kind {
                metaflac::ErrorKind::Io(error) => error,
                metaflac::ErrorKind::StringDecoding(utf8error) => {
                    io::Error::new(ErrorKind::InvalidData, utf8error)
                }
                metaflac::ErrorKind::InvalidInput => io::Error::new(ErrorKind::InvalidData, error),
            })
            .or_warn("saving tags to file")?;
        Ok(())
    }

    fn discno(&self) -> Option<String> {
        self.get("DISCNUMBER").map(|n| n.join("-"))
    }

    fn total_discs(&self) -> Option<String> {
        self.get("TOTALDISCS")
            .or_else(|| self.get("DISCTOTAL"))
            .map(|n| n.join("-"))
    }

    fn full_album(&self) -> Option<String> {
        self.album().map(|titles| {
            let discno = self
                .discno()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or_default();
            let discs = self
                .total_discs()
                .and_then(|n| n.parse::<usize>().ok())
                .unwrap_or_default();
            let title = titles.join(" - ");
            match (discno, discs) {
                (2.., _) | (_, 2..) => format!("{title} [Disc {discno}]"),
                _ => title,
            }
        })
    }

    fn set_discno(&mut self, discno: usize) {
        self.set("DISCNUMBER", vec![discno.to_string()]);
    }

    fn set_total_discs(&mut self, discs: usize) {
        self.set("TOTALDISCS", vec![discs.to_string()]);
        self.set("DISCTOTAL", vec![discs.to_string()]);
    }
}

pub trait ExtendVorbisTag {
    const KEY: &'static str;
    fn extend_vorbis(&self, vorbis: &mut VorbisComment);
}

impl ExtendVorbisTag for ReleaseStatus {
    const KEY: &'static str = "RELEASESTATUS";

    fn extend_vorbis(&self, vorbis: &mut VorbisComment) {
        let value = match self {
            ReleaseStatus::Official => "Official",
            ReleaseStatus::Promotion => "Promotion",
            ReleaseStatus::Bootleg => "Bootleg",
            ReleaseStatus::PseudoRelease => "Pseudo-Release",
            ReleaseStatus::UnrecognizedReleaseStatus => "Other",
            _ => "Other",
        };
        vorbis.set(Self::KEY, vec![value]);
    }
}

impl ExtendVorbisTag for ReleaseScript {
    const KEY: &'static str = "SCRIPT";

    fn extend_vorbis(&self, vorbis: &mut VorbisComment) {
        vorbis.set(Self::KEY, vec![self.code()])
    }
}

/// Returns a sanitised version of the string suitable for use as a filename.
///
/// We strip out invalid / dangerous characters, based on the target, and additionally remove any
/// leading / trailing whitespace and leading `-`
fn sanitise(pathsegment: &str) -> String {
    #[cfg(windows)]
    /// Returns `true` if the character is valid for a filename on Windows.
    ///
    /// A character is considered valid if:
    /// - It is an ASCII character (`char::is_ascii()` returns `true`)
    /// - Its Unicode code point is between 32 and 126 inclusive
    /// - It is not one of the reserved filename characters: `< > : " / \ | ? *`
    fn is_valid(c: &char) -> bool {
        let reserved = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
        c.is_ascii() && *c >= ' ' && *c <= '~' && !reserved.contains(c)
    }

    #[cfg(unix)]
    /// Returns `true` if the character is valid for a (sane) filename on Posix.
    ///
    /// While technically, any character except `\0` and `/` is valid for a filename,
    /// a range of characters can cause practical and security issues, so we restrict them here.
    ///
    /// A character is considered valid if:
    /// - It is not an ASCII control code
    /// - It is not one of the reserved filename characters `\0` or `/`
    /// - It is not a character which is expanded in bash within `"` double quotes: `$ ` \ !`
    /// - .. a valid bash quote: `' "`
    /// - .. or otherwise particularly dangerous in inadvertent shell expansions: `< > | ? *`
    /// - Note that we allow all forms of parentheses so filename expansions will still need to
    ///   be quoted as per good practice
    fn is_valid(c: &char) -> bool {
        let reserved = ['\0', '/'];
        let expanded = ['$', '`', '\\', '!'];
        let quotes = ['\'', '"'];
        let dangerous = ['<', '>', '|', '?', '*'];

        !c.is_ascii_control()
            && !reserved.contains(c)
            && !expanded.contains(c)
            && !quotes.contains(c)
            && !dangerous.contains(c)
    }

    fn invalid_at_start(c: &char) -> bool {
        c.is_whitespace() || *c == '-'
    }

    pathsegment
        .chars()
        .filter(is_valid)
        .skip_while(invalid_at_start)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    mod full_album {
        use super::*;

        #[test]
        fn one_title() {
            let mut vorbis = VorbisComment::new();
            vorbis.set_album(vec!["an album"]);
            assert_eq!(vorbis.full_album().unwrap(), "an album");
        }

        #[test]
        fn two_titles() {
            let mut vorbis = VorbisComment::new();
            vorbis.set_album(vec!["an album", "with a subtitle"]);
            assert_eq!(vorbis.full_album().unwrap(), "an album - with a subtitle");
        }

        #[test]
        fn disc1_of_1() {
            let mut vorbis = VorbisComment::new();
            vorbis.set_album(vec!["an album"]);
            vorbis.set_discno(1);
            vorbis.set_total_discs(1);
            assert_eq!(vorbis.full_album().unwrap(), "an album");
        }

        #[test]
        fn disc1_of_2() {
            let mut vorbis = VorbisComment::new();
            vorbis.set_album(vec!["an album"]);
            vorbis.set_discno(1);
            vorbis.set_total_discs(2);
            assert_eq!(vorbis.full_album().unwrap(), "an album [Disc 1]");
        }

        #[test]
        fn disc2() {
            let mut vorbis = VorbisComment::new();
            vorbis.set_album(vec!["an album"]);
            vorbis.set_discno(2);
            assert_eq!(vorbis.full_album().unwrap(), "an album [Disc 2]");
        }
    }

    #[test]
    fn sanitise_paths() {
        assert_eq!(sanitise("file?name*.txt"), "filename.txt");
        assert_eq!(sanitise(" -v | nasty > /dev/null"), "v  nasty  devnull");
    }
}
