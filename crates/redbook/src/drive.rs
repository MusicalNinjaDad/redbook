//! Physical drives which can read CDs

use std::{
    io,
    path::{Path, PathBuf},
};

use crate::{Frame, TocEntries};

/// The capabilites required of a low level device which can read CDs
pub trait Device: Sized {
    /// Open the device represented by the path
    fn open<P: AsRef<Path>>(path: P) -> io::Result<Self>;

    /// Get a valid file system-path which represents the device and can be passed to
    /// [`std::fs::read_dir`] etc. to read from the device as a file-system.
    fn fs_path(&self) -> io::Result<PathBuf>;

    /// Read raw CDDA data starting at frame `start` into `buf`
    ///
    /// Returns number of frames read. Errors with `ErrorKind::Unsupported` if `buf` is not sized
    /// for an exact number of frames.
    fn read_cdda(&self, start: Frame, buf: &mut [u8]) -> io::Result<Frame>;

    /// Read the Table of Contents from the disc and provide a parsed representation as a
    /// platform-independent [TocEntries]
    fn read_toc(&self) -> io::Result<TocEntries>;

    /// Read the Table of Contents from the disc and provide a raw_bytes representation of the
    /// returned data. The exact format of this data may be platform specific.
    ///
    /// Unless you are providing low-level or debug functionality, you probably want to use [read_toc]
    /// instead which will provide a platform-independent [TocEntries]
    fn raw_toc(&self) -> io::Result<Vec<u8>>;
}
