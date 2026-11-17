use std::path::{Path, PathBuf};

use crate::{
    Frame, TocEntries,
    drive::error::{DeviceError, OsError},
    frame::ByteSize,
};

/// The capabilites required of a low level device which can read CDs
pub trait Device: Sized {
    /// An implementation-specific "coloured" io::Error returned by this device.
    ///
    /// This allows for OS-specific parsing of non-overlapping error codes
    /// (e.g. windows `ERROR_NOT_READY` == 21, vs linux `ENOMEDIUM` == 123)
    ///
    /// # Note for implementors
    /// This Error type is never actually used in the trait methods. Implementors are recommended
    /// to keep the error type private and to implement `From<MyOsError> for DeviceError` as the
    /// only available conversion in order to ensure consistency.
    ///
    /// See the docs for [`OsError`] for more details and an example.
    type OsError: OsError;

    /// The type of Iterator available that provides all available devices on the system.
    ///
    /// Items are `Result`s so that enumeration / drive-construction failures are propagated
    /// rather than silently ending iteration (issue #50): `None` is reserved for the expected
    /// end-of-enumeration condition.
    type Iter: Iterator<Item = Result<Self, DeviceError>>;

    /// Open the device represented by the path. See [AudioCd::open][crate::AudioCd::open] for
    /// details on acceptable path formats.
    fn open<P: AsRef<Path> + ?Sized>(path: &P) -> Result<Self, DeviceError>;

    /// Get an iterator over all available cd devices on the system
    fn all() -> Result<Self::Iter, DeviceError>;

    /// Get a valid file system-path which represents the device and can be passed to
    /// [`std::fs::read_dir`] etc. to read from the device as a file-system.
    ///
    /// Depending on the implementation this may not be the same value as was provided to `open`.
    /// E.g. on windows this could return `\\.\CDROM0` when `F:` was passed to `open`.
    fn fs_path(&self) -> Result<PathBuf, DeviceError>;

    /// Read raw CDDA data starting at frame `start` into `buf`
    ///
    /// `start` is the absolute frame position on the disc, including the 150-frame lead-in
    /// (as provided by [`TrackEntry::start`][crate::toc::TrackEntry::start]).
    ///
    /// Returns number of frames read. Errors with [`DeviceError::Unsupported`] if `buf` is not
    /// sized for an exact number of frames.
    fn read_cdda(&self, start: Frame, buf: &mut [u8]) -> Result<Frame, DeviceError>;

    /// Read the Table of Contents from the disc and provide a parsed representation as a
    /// platform-independent [TocEntries]. If a cached version is available then it may be cloned
    /// and returned rather than re-reading from disc.
    fn read_toc(&self) -> Result<TocEntries, DeviceError>;

    /// Read the Table of Contents from the disc and provide a raw_bytes representation of the
    /// returned data. The exact format of this data may be platform specific.
    ///
    /// Unless you are providing low-level or debug functionality, you probably want to use
    /// [read_toc][Device::read_toc] instead which will provide a platform-independent [TocEntries]
    fn raw_toc(&self) -> Result<Vec<u8>, DeviceError>;

    /// The maximum chunk size which can be read in a single call.
    ///
    /// # Note
    /// - Linux has a hard-coded limit of 75 frames
    /// - Windows has a variable limit depending on hardware type
    /// - See #224 for more details / research
    ///
    /// # Errors
    /// This may error if the value needs to be obtained at runtime and doing so fails
    fn max_chunk(&self) -> Result<ByteSize, DeviceError> {
        self.raw_max_chunk().map(ByteSize::rounded_new)
    }

    /// The maximum chunk size, in bytes, which can be read in a single call.
    ///
    /// # Note
    /// - This value will most likely *not* be a correct multiple of frames. Use the provided
    ///   [`max_chunk`][Device::max_chunk] out of preference
    /// - Linux has a hard-coded limit of 75 frames
    /// - Windows has a variable limit depending on hardware type
    /// - See #224 for more details / research
    ///
    /// # Errors
    /// This may error if the value needs to be obtained at runtime and doing so fails
    fn raw_max_chunk(&self) -> Result<usize, DeviceError>;
}
