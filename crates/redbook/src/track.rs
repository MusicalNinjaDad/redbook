//! Provides [`Track`] & associated types

use flacenc::{bitsink::MemSink, component::BitRepr, error::Verify};
use metaflac::block::{Picture, VorbisComment};

use crate::{Frame, TocEntry};

#[derive(Debug, Clone, PartialEq, Default)]
/// A track on a CD with associated metadata.
///
/// This struct represents a single track on an audio CD, combining low-level
/// TOC (Table of Contents) information with optional metadata from MusicBrainz.
///
/// # Type Parameters
///
/// * `'meta` - Lifetime of the referenced MusicBrainz metadata
///
/// # Notes
///
/// - This type is cheap to clone: all fields are `Copy` except the metadata reference
/// - The borrow checker ensures metadata validity for lifetime `'meta'`
/// - Typically constructed as `<'static>` and then cloned when referencing metadata
/// - The `meta` field is private; use [`meta()`](method@Self::meta) to access it
///
/// # Examples
///
///  TODO New docs
pub struct Track<'meta> {
    /// TOC entry for this track, containing track number and start position.
    pub toc_entry: TocEntry,
    /// Duration of this track in frames.
    pub duration: Frame,
    /// Windows-specific track identifier (from CDROM_TOC).
    pub windows_identifier: Option<u32>,
    /// Optional reference to MusicBrainz track metadata.
    pub(crate) meta: Option<&'meta musicbrainz_rs::entity::release::Track>,
}

impl Track<'static> {
    /// Create a new `Track` with no MusicBrainz metadata
    pub fn new(
        track_number: u8,
        start: Frame,
        duration: Frame,
        windows_identifier: Option<u32>,
    ) -> Self {
        let toc_entry = TocEntry {
            track: track_number,
            start,
        };
        Self {
            toc_entry,
            duration,
            windows_identifier,
            meta: None,
        }
    }
}

impl<'meta> Track<'meta> {
    /// Returns the 1-indexed track number.
    ///
    /// # Examples
    ///
    ///  TODO New docs
    pub fn track_number(&self) -> u8 {
        self.toc_entry.track
    }

    /// Returns the track title from MusicBrainz metadata, if available.
    ///
    /// # Returns
    ///
    /// `Some(String)` if MusicBrainz metadata is loaded and contains a title,
    /// `None` otherwise.
    ///
    /// # Examples
    ///
    ///  TODO New docs
    pub fn title(&self) -> Option<String> {
        self.meta.map(|track| track.title.clone())
    }

    /// Returns the most likely representation of the track listing, as we expect it was
    /// written on the back of the CD. The only adjustments we make are to ensure that numerical
    /// track numbers are always 2 digits long, in order to allow alphabetical sorting to work.
    ///
    /// # Returns
    ///
    /// A string suitable for use as a filename, e.g., "05 Columbia", "A1 Speak to Me"
    ///
    /// # Notes
    ///
    /// - Uses the text representation for track number from MusicBrainz if available
    /// - Falls back to the two-digit track number if MusicBrainz data is not available
    /// - Numerical track numbers are always formatted as 2 digits (e.g., "05" instead of "5")
    ///
    /// # Examples
    ///
    ///  TODO New docs
    pub fn filename(&self) -> String {
        let track_num = self
            .meta()
            .map(|trk| {
                let trk_num = trk.number.clone();
                match trk_num.len() {
                    1 if trk_num.parse::<usize>().is_ok() => format!("0{trk_num}"),
                    _ => trk_num,
                }
            })
            .unwrap_or_else(|| format!("{:02}", self.toc_entry.track));
        [track_num, self.title().unwrap_or_default()].join(" ")
    }

    /// Returns a reference to the MusicBrainz track metadata, if available.
    ///
    /// # Returns
    ///
    /// `Some(&musicbrainz::Track)` if MusicBrainz metadata is loaded for this track,
    /// `None` otherwise.
    ///
    /// # Notes
    ///
    /// - This provides access to the full MusicBrainz track data, including artist, album, etc.
    /// - The lifetime of the returned reference is tied to the `'meta` lifetime parameter
    ///
    /// # Examples
    ///
    ///  TODO New docs
    pub fn meta(&self) -> Option<&'meta musicbrainz_rs::entity::release::Track> {
        self.meta
    }
}

/// A ripped audio track containing raw CD data and metadata.
///
/// This struct wraps the raw audio data from a CD track along with its track number.
/// It provides methods for encoding the raw data to various formats.
///
/// # Notes
///
/// - The raw data is in CDDA format (2352 bytes per frame)
/// - Use [`to_flac`](method@Self::to_flac) or [`to_wav`](method@Self::to_wav) to encode to compressed or uncompressed formats
///
/// # Examples
///
///  TODO New docs
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
    /// # Returns
    ///
    /// A [`MemSink<u8>`] containing the FLAC-encoded audio data.
    ///
    /// # Notes
    ///
    /// - The audio is encoded as 16-bit stereo at 44.1 kHz
    /// - FLAC is a lossless compression format, typically reducing CD audio to ~60% of original size
    /// - The returned type can be converted to a byte vector using `.into_inner()`
    ///
    /// # Examples
    ///
    ///  TODO New docs
    pub fn to_flac(&self) -> MemSink<u8> {
        let (channels, bits_per_sample, sample_rate) = (2, 16, 44100);
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .expect("Config data error.");
        #[expect(
            clippy::chunks_exact_to_as_chunks,
            reason = "TODO error handling if not exact"
        )]
        let samples: Vec<_> = self
            .raw_data
            .chunks_exact(2)
            .map(|chunk| i16::from_le_bytes([chunk[0], chunk[1]]) as i32)
            .collect();
        let source = flacenc::source::MemSource::from_samples(
            &samples,
            channels,
            bits_per_sample,
            sample_rate,
        );
        let flac_stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .expect("Encode failed.");
        let mut sink = flacenc::bitsink::ByteSink::new();
        flac_stream.write(&mut sink).unwrap();
        sink
    }

    /// Encodes the raw CD audio data to WAV format.
    ///
    /// # Returns
    ///
    /// A byte vector containing the WAV-encoded audio data.
    ///
    /// # Notes
    ///
    /// - The audio is encoded as 16-bit stereo at 44.1 kHz (standard CD audio)
    /// - WAV is an uncompressed format, so the output will be the same size as the input
    /// - The WAV header is written with the correct RIFF format specifications
    /// - Inspired by implementations from the rust-cd-da-reader project
    ///
    /// # Examples
    ///
    ///  TODO New docs
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
