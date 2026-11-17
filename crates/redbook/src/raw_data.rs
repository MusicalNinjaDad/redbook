//! TODO

use std::{
    ops::{Deref, DerefMut},
    slice, vec,
};

use crate::Frame;

/// Raw data for CDDA audio. Guaranteed to be sized to a multiple of Frames, provides simple interconversion
/// to flacenc, always properly initialised for use as a buffer in unsafe code.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RawData(Box<[u8]>);

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

impl IntoIterator for RawData {
    type Item = u8;

    type IntoIter = vec::IntoIter<u8>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
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
    const CHANNELS: usize = 2;
    /// The redbook standard defined 16-bits per sample
    const BITRATE: usize = 16;
    /// The redbook standard defined rate of 44100 Hz (samples per second)
    const SAMPLERATE: usize = 44100;

    /// Create a new raw data for the given number of frames
    pub fn new(size: Frame) -> Self {
        let zeroes = Box::<[u8]>::new_zeroed_slice(size.size().strict_cast());
        // SAFETY: \0 is valid for u8. Zeroed is therefore safe to assume_init
        let inner = unsafe { zeroes.assume_init() };

        Self(inner)
    }
}
