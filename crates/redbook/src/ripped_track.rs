//! A ripped audio track containing raw CD data and metadata.

use std::{
    fs,
    path::{Path, PathBuf},
};

use flacenc::{bitsink::MemSink, component::BitRepr, error::Verify};
use metaflac::block::{Picture, PictureType, VorbisComment};

use crate::{
    error::{EncodingError, MetaDataError, UnknownMimeType},
    raw_data::RawData,
    tags::{
        artwork::picture_from_jpeg,
        vorbis::{PictureExt, VorbisTagExt},
    },
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
    pub raw_data: RawData,
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
        let config = flacenc::config::Encoder::default().into_verified()?;
        let source = self.raw_data.as_source();
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
        let pcm_data_size = pcm.len().cast::<usize>();
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
