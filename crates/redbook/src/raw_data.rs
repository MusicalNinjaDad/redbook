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
    /// Create a new raw data for the given number of frames
    pub fn new(size: Frame) -> Self {
        let zeroes = Box::<[u8]>::new_zeroed_slice(size.size().strict_cast());
        // SAFETY: \0 is valid for u8. Zeroed is therefore safe to assume_init
        let inner = unsafe { zeroes.assume_init() };

        Self(inner)
    }
}
