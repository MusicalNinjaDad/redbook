//! A ripped audio track containing raw CD data and metadata.

use flacenc::{bitsink::MemSink, component::BitRepr, error::Verify};
use metaflac::block::{Picture, VorbisComment};

use crate::EncodingError;

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
}
