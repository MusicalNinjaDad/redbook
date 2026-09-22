#![expect(missing_docs, reason = "needs update")]
//! Tagging utilities for FLAC metadata

use std::path::PathBuf;

use metaflac::block::{Picture, PictureType, VorbisComment};
use musicbrainz_rs::entity::{
    artist_credit::ArtistCredit,
    release::{Release, ReleaseStatus, Track},
    release_scripts::ReleaseScript,
};
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
            mime_type: "image/jpeg".to_string(),
            description: description.to_string(),
            width,
            height,
            depth: 24,
            num_colors: 0,
            data: data.as_ref().to_vec(),
        }
    }
}

pub trait VorbisTagExt {
    /// 0n Full - title
    fn filename(&self) -> PathBuf;

    /// Album - Artists/Album title [Disc n]
    fn directory(&self) -> PathBuf;

    /// Title one - Title two
    fn full_title(&self) -> String;
}

impl VorbisTagExt for VorbisComment {
    fn filename(&self) -> PathBuf {
        let track_number = format!("{:02}", self.track().unwrap_or_default());
        let title = self.full_title();
        PathBuf::from(sanitise(&[track_number, title].join(" ")))
    }

    fn directory(&self) -> PathBuf {
        let artist = self.album_artist().map(|artists| artists.join(" ")).unwrap_or_else(|| "Unknown artist".to_string());
        let title = self.album().map(|titles| titles.join(" ")).unwrap_or_else(|| "Unknown album".to_string());
        PathBuf::from(sanitise(&artist)).join(sanitise(&title))
    }

    fn full_title(&self) -> String {
        self.title()
            .map(|titles| titles.join(" - "))
            .unwrap_or_default()
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

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn sanitise_paths() {
        assert_eq!(sanitise("file?name*.txt"), "filename.txt");
        assert_eq!(sanitise(" -v | nasty > /dev/null"), "v  nasty  devnull");
    }
}
