//! Physical drives which can read CDs

use std::{
    fmt::Debug,
    io,
    path::{Path, PathBuf},
};

use thread_safely::Context;

use crate::{Frame, RipProgress, TocEntries};
/// The capabilites required of a low level device which can read CDs
pub trait Device: Sized {
    /// Open the device represented by the path
    fn open<P: AsRef<Path>>(path: P) -> io::Result<Self>;

    /// Get a valid file system-path which represents the device and can be passed to
    /// [`std::fs::read_dir`] etc. to read from the device as a file-system.
    ///
    /// Depending on the implementation this may not be the same value as was provided to `open`.
    /// E.g. on windows this could return `\\.\CDROM0` when `F:` was passed to `open`.
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

/// Implementors of `Drive` can additionally read an entire track and may hold a [`Context<RipProgress>`]
/// to allow for mid-rip termination and status updates.
///
/// Implementors of `Drive` are more likely to store cached versions of data such as their path and
/// ToC, where implementors of only [`Device`] are likely to be low-level entities which directy access
/// hardware.
pub trait Drive: Device {
    /// Read an entire track of data allowing cancellation mid-read and providing status updates
    /// via the thread [Context].
    fn read_track(&self, track: usize) -> io::Result<Vec<u8>>;

    /// The path for the drive. If you don't need an owned value then, unlike
    /// [`fs_path`][Device::fs_path], this will avoid a heap allocation and will never error.
    fn path(&self) -> &Path;

    /// Constructor for multi-threaded applications. Allows a thread [Context] to be provided
    /// so that long-running reads via [rip] & [read_track] can be cancelled and provide status
    /// updates.
    fn with_context<P: AsRef<Path>>(path: P, cx: Context<RipProgress>) -> io::Result<Self>;

    /// Add a thread [Context] to an existing `AudioCd`
    fn add_context(&mut self, cx: Context<RipProgress>);

    /// Retrieve the thread [Context]. If no context was provided via [with_context]
    /// or [add_context] then this will be a dummy, default context.
    fn cx(&self) -> Context<RipProgress>;
}

#[derive(Debug, Clone)]
struct CdDrive<DEV: Device> {
    path: PathBuf,
    device: DEV,
    tocentries: TocEntries,
    cx: Context<RipProgress>,
}

impl<DEV: Device + Debug> Device for CdDrive<DEV> {
    fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let device = DEV::open(&path)?;
        let path = PathBuf::from(path.as_ref());
        let tocentries = device.read_toc()?;
        let cx = Default::default();
        Ok(Self {
            path,
            device,
            tocentries,
            cx,
        })
    }

    fn fs_path(&self) -> io::Result<PathBuf> {
        Ok(self.path.clone())
    }

    fn read_cdda(&self, start: Frame, buf: &mut [u8]) -> io::Result<Frame> {
        self.device.read_cdda(start, buf)
    }

    fn read_toc(&self) -> io::Result<TocEntries> {
        self.device.read_toc()
    }

    fn raw_toc(&self) -> io::Result<Vec<u8>> {
        self.device.raw_toc()
    }
}
