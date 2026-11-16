use std::path::{Path, PathBuf};

use metaflac::{
    Tag,
    block::{Picture, PictureType, VorbisComment},
};
use tracing_result::Trace;

use crate::error::{MetaDataError, UnknownMimeType};

use super::{artwork::ImageMimeTypes, filenames::sanitise};

/// File naming logic for the cover art [`Picture`] of a [`RippedTrack`].
pub(crate) trait PictureExt {
    /// The filename for the picture, based on its picture type & mime type (e.g. `cover.jpg`)
    fn filename(&self) -> Result<PathBuf, UnknownMimeType>;
}

impl PictureExt for Picture {
    fn filename(&self) -> Result<PathBuf, UnknownMimeType> {
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

/// File & directory naming logic for the [`VorbisComment`] tags of a [`RippedTrack`].
pub(crate) trait VorbisTagExt {
    /// 0n Full - title
    fn filename(&self) -> PathBuf;

    /// Album - Artists/Album title [Disc n]
    fn directory(&self) -> PathBuf;

    /// Title one - Title two
    fn full_title(&self) -> String;

    /// Write tags to the given file
    fn add_to<P: AsRef<Path>>(self, path: &P) -> Result<(), MetaDataError>;

    /// The `DISCNUMBER` tag, if present
    fn discno(&self) -> Option<String>;

    /// The `TOTALDISCS`/`DISCTOTAL` tag, if present
    fn total_discs(&self) -> Option<String>;

    /// Set the `DISCNUMBER` tag
    #[cfg(test)]
    fn set_discno(&mut self, discno: usize);

    /// Set the `TOTALDISCS` & `DISCTOTAL` tags
    #[cfg(test)]
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

    fn add_to<P: AsRef<Path>>(self, path: &P) -> Result<(), MetaDataError> {
        let mut file_tag = Tag::read_from_path(path.as_ref()).or_warn("reading tags from file")?;
        file_tag
            .vorbis_comments_mut()
            .comments
            .extend(self.comments);
        file_tag
            .write_to_path(path.as_ref())
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

    #[cfg(test)]
    fn set_discno(&mut self, discno: usize) {
        self.set("DISCNUMBER", vec![discno.to_string()]);
    }

    #[cfg(test)]
    fn set_total_discs(&mut self, discs: usize) {
        self.set("TOTALDISCS", vec![discs.to_string()]);
        self.set("DISCTOTAL", vec![discs.to_string()]);
    }
}

#[cfg(test)]
mod tests {
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
