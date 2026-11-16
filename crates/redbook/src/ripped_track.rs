//! A ripped audio track containing raw CD data and metadata.

use std::{
    fs,
    path::{Path, PathBuf},
};

use flacenc::{bitsink::MemSink, component::BitRepr, error::Verify};
use metaflac::{
    Tag,
    block::{Picture, PictureType, VorbisComment},
};
use tracing_result::Trace;

use crate::{
    disc::tagging::picture_from_jpeg,
    error::{EncodingError, MetaDataError, UnknownMimeType},
    tags::{filenames::sanitise, vorbis::PictureExt},
};

/// This struct wraps the raw audio data from a CD track along with a full set of vorbis comments
/// (tags) and the cover art.
/// It provides methods for encoding the raw data to various formats.
///
/// # Notes
///
/// - The raw data is in CDDA format (2352 bytes per frame)
/// - Use [`to_flac`](method@Self::to_flac) or [`to_wav`](method@Self::to_wav) to encode
#[derive(Debug, Clone)]
pub struct RippedTrack {
    /// Track details
    pub tags: VorbisComment,
    /// Cover art
    pub coverart: Option<Picture>,
    /// Raw CD audio data (2352 bytes per frame).
    pub raw_data: Vec<u8>,
}

impl RippedTrack {
    /// The full title of the track: multiple `TITLE` tags joined with `" - "`
    /// (e.g. "Title one - Title two").
    pub fn full_title(&self) -> String {
        self.tags.full_title()
    }

    /// The base filename for the track's audio file: `"0n Full - title"`, without extension.
    pub fn filename(&self) -> PathBuf {
        self.tags.filename()
    }

    /// The directory for the album: `Artist/Album title [Disc n]`.
    pub fn directory(&self) -> PathBuf {
        self.tags.directory()
    }

    /// The filename for the cover art (e.g. `cover.jpg`), if cover art is present.
    ///
    /// # Errors
    ///
    /// Returns [`UnknownMimeType`] if the cover art's mime type is not recognised.
    pub fn coverart_filename(&self) -> Option<Result<PathBuf, UnknownMimeType>> {
        self.coverart.as_ref().map(|cover| cover.filename())
    }

    /// Encodes the raw CD audio data to FLAC format.
    ///
    /// # Errors
    /// - [`TrackError::InvalidData`] if the raw data is not a whole number of samples
    /// - [`TrackError::Encode`] / [`TrackError::Output`] wrapping the underlying
    ///   [`flacenc`] errors if encoding fails
    pub fn to_flac(&self) -> Result<MemSink<u8>, EncodingError> {
        let (channels, bits_per_sample, sample_rate) = (2, 16, 44100);
        let config = flacenc::config::Encoder::default().into_verified()?;
        (self.raw_data.len() % 2 == 0).ok_or(EncodingError::InvalidData(self.raw_data.len()))?;
        let samples: Vec<_> = self
            .raw_data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as i32)
            .collect();
        let source = flacenc::source::MemSource::from_samples(
            &samples,
            channels,
            bits_per_sample,
            sample_rate,
        );
        let flac_stream =
            flacenc::encode_with_fixed_block_size(&config, source, config.block_size)?;
        let mut sink = flacenc::bitsink::ByteSink::new();
        flac_stream.write(&mut sink)?;
        Ok(sink)
    }

    /// Encodes the raw CD audio data to WAV format.
    pub fn to_wav(&self) -> Vec<u8> {
        let pcm = &self.raw_data;

        // based on https://github.com/Bloomca/rust-cd-da-reader/blob/fd71208262c199dc44d8a012731be298a848ea79/src/lib.rs#L226
        // & https://github.com/Bloomca/rust-cd-da-reader/blob/main/src/utils.rs#L49
        let pcm_data_size = pcm.len();
        let mut wav = Vec::with_capacity(44 + pcm_data_size);
        let pcm_data_size = pcm_data_size as u32;

        // RIFF header
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&(pcm_data_size + 36).to_le_bytes()); // file size - 8
        wav.extend_from_slice(b"WAVE");

        // fmt chunk
        wav.extend_from_slice(b"fmt ");
        wav.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
        wav.extend_from_slice(&1u16.to_le_bytes()); // PCM format
        wav.extend_from_slice(&2u16.to_le_bytes()); // channels
        wav.extend_from_slice(&44100u32.to_le_bytes()); // sample rate
        wav.extend_from_slice(&176400u32.to_le_bytes()); // byte rate
        wav.extend_from_slice(&4u16.to_le_bytes()); // block align
        wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

        // data chunk header
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&pcm_data_size.to_le_bytes());

        wav.extend(pcm);
        wav
    }

    /// Adds the track's tags to the vorbis comments of the given (flac) file, extending any
    /// comments already present.
    ///
    /// # Errors
    ///
    /// Returns [`TaggingError`] if the file's tags cannot be read or written.
    pub fn add_tags_to<P: AsRef<Path>>(&self, path: &P) -> Result<(), MetaDataError> {
        self.tags.clone().add_to(path)
    }

    /// The cover art for the track: the cached art if present, otherwise a front cover
    /// constructed from the jpeg file at `fallback`.
    pub fn coverart_or_from_file<P: AsRef<Path>>(&self, fallback: P) -> Option<Picture> {
        self.coverart.clone().or_else(|| {
            fs::read(fallback)
                .ok()
                .map(|data| picture_from_jpeg(PictureType::CoverFront, "Front Cover", data))
        })
    }
}

/// File & directory naming logic for the [`VorbisComment`] tags of a [`RippedTrack`].
trait VorbisTagExt {
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
