//! Provides [`Track`] & associated types

use std::fmt::Display;

use flacenc::{bitsink::MemSink, component::BitRepr, error::Verify};
use metaflac::block::{Picture, VorbisComment};

use crate::{
    Frame,
    toc::{Control, TrackEntry, TrackNo},
};

pub mod error;

pub use error::{TrackError, TrackErrorKind};

#[derive(Debug, Copy, Clone, PartialEq)]
/// A track on a CD with associated metadata.
///
/// This struct represents a single track on an audio CD, combining low-level
/// TOC (Table of Contents) information with optional metadata from MusicBrainz.
///
/// # Notes
///
/// - Typically constructed as `<'static>` and then cloned when referencing metadata
pub struct Track<'meta> {
    /// TOC entry for this track, containing track number and start position.
    pub toc_entry: TrackEntry,
    /// Duration of this track in frames.
    pub duration: Frame,
    /// Optional reference to MusicBrainz track metadata.
    pub(crate) meta: Option<&'meta musicbrainz_rs::entity::release::Track>,
}

impl<'meta> Display for Track<'meta> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{toc}, duration: {dur}, has metadata: {meta}",
            toc = self.toc_entry,
            dur = self.duration,
            meta = self.meta.is_some()
        )
    }
}

impl Track<'static> {
    /// Create a new `Track` with no MusicBrainz metadata
    ///
    /// # Notes
    /// - `track_number` needs no validation: a [`TrackNo`] is always a valid track number
    ///   (1..=99). "Special" POINT values (e.g. 0xA0..=0xA2, 0xAA) are
    ///   [`Point`][crate::toc::Point]s, not track numbers
    pub fn new(track_number: TrackNo, start: Frame, duration: Frame) -> Self {
        Self {
            toc_entry: TrackEntry::new(track_number, start, Control::default(), None),
            duration,
            meta: None,
        }
    }
}

impl<'meta> Track<'meta> {
    /// Returns the 1-indexed track number.
    pub fn track_number(&self) -> TrackNo {
        self.toc_entry.trackno()
    }

    /// Returns the track title from MusicBrainz metadata, if available.
    pub fn title(&self) -> Option<String> {
        self.meta.map(|track| track.title.clone())
    }

    /// Returns the most likely representation of the track listing, as we expect it was
    /// written on the back of the CD. The only adjustments we make are to ensure that numerical
    /// track numbers are always 2 digits long, in order to allow alphabetical sorting to work.
    ///
    /// # Notes
    ///
    /// - Uses the text representation for track number from MusicBrainz if available
    /// - Falls back to the two-digit track number if MusicBrainz data is not available
    /// - Numerical track numbers are always formatted as 2 digits (e.g., "05")
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
            .unwrap_or_else(|| format!("{:02}", self.toc_entry.trackno()));
        [track_num, self.title().unwrap_or_default()].join(" ")
    }

    /// Returns a reference to the MusicBrainz track metadata, if available.
    pub fn meta(&self) -> Option<&'meta musicbrainz_rs::entity::release::Track> {
        self.meta
    }
}

impl PartialEq<cdtoc::Track> for Track<'_> {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (&self.toc_entry == toc_track) && (self.duration == toc_track.duration())
    }
}

/// A ripped audio track containing raw CD data and metadata.
///
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
    /// - [`TrackErrorKind::InvalidData`] if the raw data is not a whole number of samples
    /// - [`TrackErrorKind::Encode`] / [`TrackErrorKind::Output`] wrapping the underlying
    ///   [`flacenc`] errors if encoding fails
    pub fn to_flac(&self) -> Result<MemSink<u8>, TrackError> {
        let (channels, bits_per_sample, sample_rate) = (2, 16, 44100);
        let config = flacenc::config::Encoder::default()
            .into_verified()
            .expect("Config data error.");
        (self.raw_data.len() % 2 == 0).ok_or_else(|| {
            TrackError::invalid_data().context("raw data is not a whole number of samples")
        })?;
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
        let flac_stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
            .map_err(TrackError::from)?;
        let mut sink = flacenc::bitsink::ByteSink::new();
        flac_stream.write(&mut sink).map_err(TrackError::from)?;
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
