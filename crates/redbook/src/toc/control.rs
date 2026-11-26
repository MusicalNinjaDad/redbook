//! Provides [`Control`] to handle TOC entry control-codes

use std::fmt::Display;

use bitflags::bitflags;
use derive_more::Display;

/// Q Sub-Channel control flags, as per table 222 of MMC-3 draft (section 5.22.2)
///
/// # Interpretation
/// | Value |  | Definition |
/// |------:|--|:-----------|
/// | 00x0b |  | 2 audio channels without pre-emphasis |
/// | 00x1b |  | 2 audio channels with pre-emphasis of 50/15 µs |
/// | 10x0b |  | audio channels without pre-emphasis (reserved in CD-R/RW) |
/// | 10x1b |  | audio channels with pre-emphasis of 50/15 µs (reserved in CD-R/RW) |
/// | 01x0b |  | Data track, recorded uninterrupted |
/// | 01x1b |  | Data track, recorded incremental |
/// | 11xxb |  | Reserved |
/// | xx0xb |  | digital copy prohibited |
/// | xx1xb |  | digital copy permitted |
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, PartialOrd, Ord)]
pub struct Control(u8);

bitflags! {
        impl Control: u8 {
        /// pre-emphasis of 50/15 µs
        const PRE_EMPHASIS = 0b0001;
        /// digital copy-permitted
        const COPY_PERMITTED = 0b0010;
        /// data track
        const DATA_TRACK = 0b0100;
        /// non-stereo audio
        /// (never used commerically, ignored as a likely candidate for "copy-protection" breakage)
        const NON_STEREO_AUDIO = 0b1000;
    }
}

impl Display for Control {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        bitflags::parser::to_writer_strict(self, f)
    }
}

/// Whether an entry belongs to an audio or a data track, as indicated by the
/// [`Control::DATA_TRACK`] bit of its [`Control`] code.
///
/// Returned by [`Ctrl::kind`]. Prefer `match`ing on this over branching on the
/// [`is_audio`][Ctrl::is_audio] / [`is_data`][Ctrl::is_data] booleans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum TrackKind {
    /// An audio track: the [`Control::DATA_TRACK`] bit is not set
    Audio,
    /// A data track: the [`Control::DATA_TRACK`] bit is set
    Data,
}

/// Contains a `Control` bitfield which can be queried
pub trait Ctrl {
    /// obtain the relevant Control bitfield
    fn control(&self) -> Control;

    /// whether this is an audio or a data track
    fn track_kind(&self) -> TrackKind {
        match self.control().contains(Control::DATA_TRACK) {
            true => TrackKind::Data,
            false => TrackKind::Audio,
        }
    }

    /// represents a data track
    fn is_data(&self) -> bool {
        matches!(self.track_kind(), TrackKind::Data)
    }

    /// represents an audio track
    fn is_audio(&self) -> bool {
        matches!(self.track_kind(), TrackKind::Audio)
    }
}

impl Ctrl for Control {
    fn control(&self) -> Control {
        *self
    }
}
