//! TODO

use crate::Frame;

/// Raw data for CDDA audio. Guaranteed to be sized to a multiple of Frames, provides simple interconversion
/// to flacenc, always properly initialised for use as a buffer in unsafe code.
pub struct RawData(Box<[u8]>);

impl RawData {
    /// Create a new raw data for the given number of frames
    pub fn new(size: Frame) -> Self {
        let zeroes = Box::<[u8]>::new_zeroed_slice(size.size().strict_cast());
        // SAFETY: \0 is valid for u8. Zeroed is therefore safe to assume_init
        let inner = unsafe { zeroes.assume_init() };

        Self(inner)
    }
}
