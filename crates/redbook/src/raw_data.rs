//! Provides [`RawData`] - a typesafe representation of the audio data pulled from a CD that upholds
//! additional variants for usage in unsafe code

use std::{
    cmp::min,
    ops::{Deref, DerefMut},
    pin::Pin,
    slice,
};

use crate::{Frame, frame::ByteSize};

/// Raw data for CDDA audio.
///
/// - guaranteed to be sized to a multiple of [`Frame`]s
/// - provides clone-free usage in [`flacenc`]
/// - always properly initialised for use as a buffer in unsafe code
/// - the underlying data is pinned in place, therefore may be used for non-blocking IO (e.g.
///   "overlapped IO" on Windows), subject the the usual aliasing rules
/// - de-references to [`&[u8]`][slice]
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawData(Pin<Box<[u8]>>);

impl !Unpin for RawData {}

impl Deref for RawData {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for RawData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<'a> IntoIterator for &'a RawData {
    type Item = &'a u8;

    type IntoIter = slice::Iter<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

impl<'a> IntoIterator for &'a mut RawData {
    type Item = &'a mut u8;

    type IntoIter = slice::IterMut<'a, u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter_mut()
    }
}

impl RawData {
    /// We expect all CD audio to be 2 channel
    ///
    /// # Note
    /// The ISO version of redbook (IEC 60908 1998) defines the MSB of the [`CONTROL`][crate::toc::Control]
    /// as specifying "broadcasting use" when set (para 17.5 note 2). The MMC spec simply states
    /// "audio channels" when set vs "2 audio channels" when unset. Philips/Sony apparently published
    /// proprietary standards where 4-channel audio was possible but such CDs were never commerically
    /// produced.
    pub const CHANNELS: usize = 2;
    /// The redbook standard defined 16-bits per sample
    pub const BITRATE: usize = 16;
    /// The redbook standard defined rate of 44100 Hz (samples per second)
    pub const SAMPLERATE: usize = 44100;

    /// Create a new raw data for the given number of frames.
    ///
    /// Once created `RawData` is not resizable
    pub fn new(size: Frame) -> Self {
        // This compiles to the same as `unsafe { Box::new_zeroed_slice(n).assume_init()}`
        // so no risk of re-allocations. Thanks rustc & Morgane55440!!
        // https://users.rust-lang.org/t/when-is-the-memory-location-of-box-u8-guaranteed-to-remain-stable/143745/7
        let inner = vec![0; size.size().strict_cast()].into_boxed_slice();

        Self(Box::into_pin(inner))
    }

    /// The size of the underlying buffer
    pub fn len(&self) -> ByteSize {
        ByteSize::strict_new(self.0.len())
    }

    /// The number of [`Frame`]s that can be held in the underlying buffer
    pub fn frames(&self) -> Frame {
        self.len().frames()
    }

    /// Provide a [`RawDataSource`] suitable for use with [`flacenc::encode_with_fixed_block_size`]
    ///
    /// This avoids excessive the cloning required to work via a [`flacenc::source::MemSource`] by
    /// simply providing a cursor over borrowed `RawData`
    pub fn as_source<'data>(&'data self) -> RawDataSource<'data> {
        RawDataSource {
            data: self,
            current_index: Default::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// A view of a [`RawData`] suitable for use with [`flacenc::encode_with_fixed_block_size`]
///
/// This avoids excessive the cloning required to work via a [`flacenc::source::MemSource`] by
/// simply providing a cursor over borrowed `RawData`
pub struct RawDataSource<'data> {
    data: &'data RawData,
    current_index: usize = 0,
}

impl<'data> flacenc::source::Source for RawDataSource<'data> {
    /// Fixed 2 channels - see [`RawData::CHANNELS`] for explanation
    fn channels(&self) -> usize {
        RawData::CHANNELS
    }

    /// Fixed 16bits - see [`RawData::BITRATE`] for explanation
    fn bits_per_sample(&self) -> usize {
        RawData::BITRATE
    }

    /// Fixed 44100 - see [`RawData::SAMPLERATE`] for explanation
    fn sample_rate(&self) -> usize {
        RawData::SAMPLERATE
    }

    /// Reads `block_size` *samples per channel* into `dest` as interleaved, le-encoded u16 values
    ///
    /// The tests for RawDataSource include a worked example if you need to understand exaclty how
    /// the data is interpreted.
    fn read_samples<F: flacenc::source::Fill>(
        &mut self,
        // The implementation for MemSource shows that this is in "samples per channel"
        block_size: usize,
        dest: &mut F,
    ) -> Result<usize, flacenc::error::SourceError> {
        let bytes_per_sample = 2; // 2 x u8 gives 1 x u16 sample
        let bytes_to_read = block_size.strict_mul(self.channels().strict_mul(bytes_per_sample));

        let start = self.current_index;
        let end = min(start.strict_add(bytes_to_read), self.data.deref().len());
        let block = &self.data[start..end];

        dest.fill_le_bytes(block, bytes_per_sample)?;

        let sample_count_per_channel = block
            .len()
            .strict_div(bytes_per_sample)
            .strict_div(self.channels());

        self.current_index = end;
        Ok(sample_count_per_channel)
    }
}

#[cfg(test)]
mod tests {
    use flacenc::source::{Context, Source};
    use md5::Digest;

    use super::*;

    #[test]
    fn read_samples() {
        let data = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
        ];

        // The data will be interpreted as interleaved, le-encoded 16bit samples:
        let _samples_left = [0x0201, 0x0605, 0x0A09];
        let _samples_right = [0x0403, 0x0807, 0x0C0B];

        let rawdata = RawData(Box::pin(data));

        // Reading into a Context for the example as this allows verification of contents via MD5 digest
        let mut dest = Context::new(16, 2);
        let samples_read = rawdata.as_source().read_samples(3, &mut dest).unwrap();

        assert_eq!(samples_read, 3);
        assert_eq!(dest.total_samples(), 3);
        let md5_digest = md5::Md5::new_with_prefix(data).finalize();
        assert_eq!(dest.md5_digest(), md5_digest);
    }
}
