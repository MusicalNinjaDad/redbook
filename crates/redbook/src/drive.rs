//! Physical drives which can read CDs

use std::{
    fmt::Debug,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use thread_safely::Context;
use tracing::field::Empty;
use tracing_result::Trace;

use crate::{FRAME_SIZE, Frame, MAX_CHUNK_BYTES, MAX_CHUNK_FRAMES, RipProgress, TocEntries};
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
    /// platform-independent [TocEntries]. If a cached version is available then it may be cloned
    /// and returned rather than re-reading from disc.
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
    /// Constructor for multi-threaded applications. Allows a thread [Context] to be provided
    /// so that long-running reads via [rip] & [read_track] can be cancelled and provide status
    /// updates.
    fn with_context<P: AsRef<Path>>(path: P, cx: Context<RipProgress>) -> io::Result<Self>;

    /// Add a thread [Context] to an existing `AudioCd`
    fn add_context(&mut self, cx: Context<RipProgress>);

    /// Retrieve the thread [Context]. If no context was provided via [with_context]
    /// or [add_context] then this will be a dummy, default context.
    fn cx(&self) -> Context<RipProgress>;

    /// Read an entire track of data allowing cancellation mid-read and providing status updates
    /// via the thread [Context].
    fn read_track(&self, track_number: usize) -> io::Result<Vec<u8>> {
        let _err_span = tracing::error_span!("", track_number).entered();
        let dbg_span = tracing::debug_span!("CdDrive::read_track", track_details = Empty).entered();
        let trace_span = tracing::trace_span!("", frames_read = Empty).entered();

        tracing::info!("reading...");

        let track = self
            .toc()
            .track(track_number)
            .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "invalid track number"))
            .or_warn("")?;

        dbg_span.record("track_details", format_args!("{track}"));

        let track_size = track
            .duration
            .as_usize()
            .checked_mul(FRAME_SIZE)
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::FileTooLarge,
                    format!(
                        "track too long. {size} bytes but can only handle {max}",
                        size = (track.duration.as_usize() as u128) * (FRAME_SIZE as u128),
                        max = usize::MAX
                    ),
                )
            })
            .or_error("")?;

        (track_size > 0)
            .ok_or_else(|| io::Error::new(ErrorKind::UnexpectedEof, "zero length track"))
            .or_error("")?;

        // Vec needs to be initialised to split into chunks. Performance cost insignificant vs IO.
        let mut data = vec![0_u8; track_size];

        let (bufs, last_buf) = data.as_chunks_mut::<MAX_CHUNK_BYTES>();
        let mut frames_read_so_far = Frame::new(0);
        trace_span.record("frames_read", format_args!("{frames_read_so_far}"));

        for (i, buf) in bufs.iter_mut().enumerate() {
            self.cx().cancelled()?;
            self.cx()
                .reply(RipProgress {
                    track_number,
                    bytes_processed: frames_read_so_far.as_usize(),
                    total_bytes: track.duration.as_usize(),
                })
                .map_err(io::Error::other)
                .or_error("sending progress update")?;

            debug_assert_eq!(
                frames_read_so_far.as_usize(),
                i.strict_mul(MAX_CHUNK_FRAMES),
                "now reading chunk {i} but have only read {frames_read_so_far} frames so far"
            );

            let start = Frame::new(i * MAX_CHUNK_FRAMES);

            debug_assert_eq!(
                start, frames_read_so_far,
                "about to read chunk {i}. We are about to read from {start}, but have only read {frames_read_so_far} frames so far"
            );

            let frames_read = self.read_cdda(start, buf)?;
            frames_read_so_far += frames_read;
            trace_span.record("frames_read", format_args!("{frames_read_so_far}"));
        }

        let start = Frame::new(bufs.len() * MAX_CHUNK_FRAMES);
        debug_assert_eq!(
            start, frames_read_so_far,
            "about to read last chunk. We are about to read from {start}, but have only read {frames_read_so_far} frames so far"
        );

        if !last_buf.is_empty() {
            self.cx().cancelled()?;
            self.cx()
                .reply(RipProgress {
                    track_number,
                    bytes_processed: frames_read_so_far.as_usize(),
                    total_bytes: track.duration.as_usize(),
                })
                .map_err(io::Error::other)
                .or_error("sending progress update")?;
            let frames_read = self.read_cdda(start, last_buf)?;
            frames_read_so_far += frames_read;
            trace_span.record("frames_read", format_args!("{frames_read_so_far}"));
        }
        self.cx()
            .reply(RipProgress {
                track_number,
                bytes_processed: frames_read_so_far.as_usize(),
                total_bytes: track.duration.as_usize(),
            })
            .map_err(io::Error::other)
            .or_error("sending progress update")?;

        Ok(data)
    }

    /// The path for the drive. If you don't need an owned value then, unlike
    /// [`fs_path`][Device::fs_path], this will avoid a heap allocation and will never error.
    fn path(&self) -> &Path;

    /// The Table of Contents. If you don't need an owned value then, unlike
    /// [`read_toc`][Device::read_toc], this will avoid a heap allocation and will never error.
    fn toc(&self) -> &TocEntries;
}

#[derive(Debug, Clone)]
/// TODO: Docs
pub struct CdDrive<DEV: Device> {
    path: PathBuf,
    device: DEV,
    tocentries: TocEntries,
    cx: Context<RipProgress>,
}

impl<DEV: Device> CdDrive<DEV> {
    /// Construct a new CdDrive from constituent parts. You will almost never want to call this directly
    /// unless implementing `From`/`TryFrom`. Use [`open`][Device::open] instead.
    pub fn new(
        path: PathBuf,
        device: DEV,
        tocentries: TocEntries,
        cx: Context<RipProgress>,
    ) -> Self {
        Self {
            path,
            device,
            tocentries,
            cx,
        }
    }

    /// Get a reference to the underlying `DEV`ice
    pub fn device(&self) -> &DEV {
        &self.device
    }
}

impl<DEV: Device> Device for CdDrive<DEV> {
    fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let _err_span =
            tracing::error_span!("CdDrive::open", path = %path.as_ref().display()).entered();
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
        let _trace_span =
            tracing::trace_span!("CdDrive::read_cdda", %start, bytes = buf.len()).entered();
        self.device.read_cdda(start, buf)
    }

    fn read_toc(&self) -> io::Result<TocEntries> {
        Ok(self.tocentries.clone())
    }

    fn raw_toc(&self) -> io::Result<Vec<u8>> {
        let _debug_span = tracing::debug_span!("CdDrive::raw_toc").entered();
        self.device.raw_toc()
    }
}

impl<DEV: Device> Drive for CdDrive<DEV> {
    fn with_context<P: AsRef<Path>>(path: P, cx: Context<RipProgress>) -> io::Result<Self> {
        let _span =
            tracing::debug_span!("CdDrive::with_context", path = %path.as_ref().display(), ?cx)
                .entered();
        let mut drive = Self::open(path)?;
        drive.add_context(cx);
        Ok(drive)
    }

    fn add_context(&mut self, cx: Context<RipProgress>) {
        self.cx = cx;
    }

    fn cx(&self) -> Context<RipProgress> {
        self.cx.clone()
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn toc(&self) -> &TocEntries {
        &self.tocentries
    }
}
