//! Provides [`RawData`] - a typesafe representation of the audio data pulled from a CD that upholds
//! additional variants for usage in unsafe code

use std::{
    borrow::{Borrow, BorrowMut},
    cmp::min,
    convert::{AsMut, AsRef},
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
///
/// # Example
/// ```
/// # use redbook::{CdDrive, Device, Drive, Frame, disc::Track, raw_data::RawData, sys::win::DriveHandle, toc::TrackNo};
/// # fn main() -> redbook::Result<()> {
/// # let track = Track::new(TrackNo::strict_new(1), Frame::strict_new(150), Frame::strict_new(7500));
/// # let drive = CdDrive::open(&redbook::test_fixtures::albums::TestAlbum::DefinitelyMaybe.assets_path(), Default::default())?;
/// # let device = drive.device();
/// let mut data = RawData::new(track.duration());
/// let bufs = data.chunks_mut(drive.max_chunk().cast());
/// let mut start = track.start();
/// for buf in bufs {
///     let frames_read = device.read_cdda(start, buf)?;
///     start = start.strict_add(frames_read);
/// }
/// # redbook::Result::Ok(()) }
/// ```
///
/// # SAFETY & raw pointers
/// Whatever you do, **DO NOT** attempt to directly cast a `RawData` into a raw pointer to a
/// bag-of-bytes. The unsafe methods [`into_raw_ptr`][Self::into_raw_ptr] and
/// [`into_raw_mut_ptr`][Self::into_raw_mut_ptr] provide a somewhat safer way to do this but also
/// explain why it is a *really bad idea* and *most likely not what you need anyway*
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

impl AsMut<[u8]> for RawData {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl AsRef<[u8]> for RawData {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl Borrow<[u8]> for RawData {
    fn borrow(&self) -> &[u8] {
        self
    }
}

impl BorrowMut<[u8]> for RawData {
    fn borrow_mut(&mut self) -> &mut [u8] {
        self
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

    /// You almost certainly do not want to do this. Converts a `RawData` into a raw pointer to
    /// the underlying bag-of-bytes.
    ///
    /// This is only offered to stop you doing something *even worse* (see below)
    ///
    /// # Why you don't want to do this
    /// - This puts you in charge of the memory previously managed by the `RawData` - see
    ///   [`Box::into_raw`] for considerations
    /// - There is *no way back* - `RawData` does not provide a `from_raw` constructor or similar
    ///
    /// # What you probably need instead
    /// - `RawData` is designed to hold a large bag of bytes representing CDDA Audio.
    /// - There are no known IoCtls which are able to read more than 1-2s of audio in one call
    ///   (See [Device::raw_max_chunk][crate::Device::raw_max_chunk]) - so passing a larger `RawData`
    ///   makes little sense
    /// - [`chunks`][core::slice::chunks] is directly available via deref to `&[u8]` and
    ///   allows for `rawdata.chunks(drive.max_chunk().cast())` which respects rust's aliasing
    ///   rules and provdes an iterable set of chunks which *can* be directly dereferenced for IoCtl
    ///
    /// # What's even worse
    /// - Whatever you do, **do not** attempt to directly dereference and recast the `RawData` via
    ///   `unsafe { &*rawdata as *const [u8] } `. The [module docs for Box][std::boxed] include
    ///   details on why this most likely leads to UB when such a pointer is passed across ffi
    ///
    /// # SAFETY
    /// - See "Why you don't want to do this"
    /// - You must correctly destroy and deallocate the memory, taking into account the unpublished
    ///   and not-guaranteed internal layout of a `RawData`, which may change in the future.
    pub unsafe fn into_raw_ptr(self) -> *const [u8] {
        Box::into_raw(Pin::into_inner(self.0))
    }

    /// You almost certainly do not want to do this. Converts a `RawData` into a raw pointer to
    /// the underlying bag-of-bytes.
    ///
    /// This is only offered to stop you doing something *even worse* (see below)
    ///
    /// # Why you don't want to do this
    /// - This removes all pinning guarantees - see [`Pin::into_inner_unchecked`] for everything you
    ///   must uphold
    /// - This puts you in charge of the memory previously managed by the `RawData` - see
    ///   [`Box::into_raw`] for considerations
    /// - There is *no way back* - `RawData` does not provide a `from_raw` constructor or similar
    ///
    /// # What you probably need instead
    /// - `RawData` is designed to hold a large bag of bytes representing CDDA Audio.
    /// - There are no known IoCtls which are able to read more than 1-2s of audio in one call
    ///   (See [Device::raw_max_chunk][crate::Device::raw_max_chunk]) - so passing a larger `RawData`
    ///   makes little sense
    /// - [`chunks`][core::slice::chunks] is directly available via deref to `&[u8]` and
    ///   allows for `rawdata.chunks(drive.max_chunk().cast())` which respects rust's aliasing
    ///   rules and provdes an iterable set of chunks which *can* be directly dereferenced for IoCtl
    ///
    /// # What's even worse
    /// - Whatever you do, **do not** attempt to directly dereference and recast the `RawData` via
    ///   `unsafe { &mut *rawdata as *mut [u8] } `. The [module docs for Box][std::boxed] include
    ///   details on why this most likely leads to UB when such a pointer is passed across ffi
    ///
    /// # SAFETY
    /// - See "Why you don't want to do this"
    /// - You must uphold all pinning guarantees
    /// - You must correctly destroy and deallocate the memory, taking into account the unpublished
    ///   and not-guaranteed internal layout of a `RawData`, which may change in the future.
    pub unsafe fn into_raw_mut_ptr(self) -> *mut [u8] {
        // SAFETY: The caller has been warned to uphold pinning guarantees
        unsafe { Box::into_raw(Pin::into_inner_unchecked(self.0)) }
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

    #[test]
    fn as_ref_and_borrow() {
        let data = [0x01, 0x02, 0x03, 0x04];
        let rawdata = RawData(Box::pin(data));

        let as_ref: &[u8] = rawdata.as_ref();
        let borrowed: &[u8] = rawdata.borrow();

        assert_eq!(as_ref, &data);
        assert_eq!(borrowed, &data);
    }

    #[test]
    fn as_mut_and_borrow_mut() {
        let data = [0x01, 0x02, 0x03, 0x04];
        let mut rawdata = RawData(Box::pin(data));

        let as_mut: &mut [u8] = rawdata.as_mut();
        as_mut[0] = 0x0A;
        let borrow_mut: &mut [u8] = rawdata.borrow_mut();
        borrow_mut[1] = 0x0B;

        assert_eq!(&rawdata[..], &[0x0A, 0x0B, 0x03, 0x04]);
    }
}
