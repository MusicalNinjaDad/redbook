//! TOC Track descriptor as provided by SCSI READ_TOC command format 0010b.
//!
//! This is the format stored for test assets, which is independently obtained via cdrtools readcd

use std::io::{self, ErrorKind};

use tracing_result::Trace;

use crate::{
    Frame, TocEntries,
    frame::Msf,
    toc::{Control, TocEntry, TrackNo},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
/// The Track Descriptor provided by MMC-3 command READ TOC format 0010b (Section 5.23.4)
///
/// # Data format
/// | byte  | contents |
/// |------:|----------:|
/// | 0     | Session Number |
/// | 1     | ADR CONTROL |
/// | 2     | ZERO |
/// | 3     | Track Number |
/// | 4     | ZERO |
/// | 5     | ZERO |
/// | 6     | ZERO |
/// | 7     | ZERO |
/// | 8     | Start Mins |
/// | 9     | Start Secs |
/// | 10    | Start Frames |
struct TrackDescriptor {
    session: u8,
    adr_control: u8,
    tno: u8,
    point: u8,
    zeros: [u8; 4],
    min: u8,
    sec: u8,
    frame: u8,
}

impl TrackDescriptor {
    fn adr(&self) -> u8 {
        (self.adr_control & 0b11110000) >> 4
    }

    fn control(&self) -> Control {
        Control::from_bits_truncate(self.adr_control)
    }

    fn from_bytes(bytes: [u8; 11]) -> Self {
        let mut iter = bytes.iter().copied();

        let session = iter.next().unwrap();
        let adr_control = iter.next().unwrap();
        let tno = iter.next().unwrap();
        let point = iter.next().unwrap();
        let zeros = iter.next_chunk().unwrap();
        let min = iter.next().unwrap();
        let sec = iter.next().unwrap();
        let frame = iter.next().unwrap();

        Self {
            session,
            adr_control,
            tno,
            point,
            zeros,
            min,
            sec,
            frame,
        }
    }
}

/// Validates data format returning:
/// - `Err` (`ErrorKind::InvalidData`) for malformed Track Descriptors
/// - `None` for adr 5 (additional info descriptors for multi-session discs)
impl TryFrom<TrackDescriptor> for Option<TocEntry> {
    type Error = io::Error;

    fn try_from(descriptor: TrackDescriptor) -> Result<Self, Self::Error> {
        match descriptor.adr() {
            1 => (),
            0 | 5 => return Ok(None),
            adr => {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    format!(
                        "invalid adr value in Track Descriptor: {adr} (valid values are `1`, `5`)"
                    ),
                ));
            }
        }

        (descriptor.tno == 0).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "invalid TNO in Track Descriptor: expected `0` got {tno}",
                    tno = descriptor.tno
                ),
            )
        })?;

        (descriptor.zeros == [0; 4]).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                format!(
                    "expected `0000` in bytes 4-7 of Track Descriptor: got {:?}",
                    descriptor.zeros
                ),
            )
        })?;

        let track = TrackNo(descriptor.point);
        let start: Frame = Msf::new(descriptor.min, descriptor.sec, descriptor.frame).into();
        let control = descriptor.control();
        Ok(Some(TocEntry {
            track,
            start,
            control,
        }))
    }
}

/// A series of bytes representing the TOC Track Descriptors as returned by READ_TOC format
/// 0b0010 (See MMC-3 10g 5.23.4)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) struct RawTocDescriptors(Vec<[u8; 11]>);

impl RawTocDescriptors {
    /// Errors if not a multiple of 11 bytes
    pub(super) fn new(bytes: Vec<u8>) -> io::Result<Self> {
        let _span = tracing::error_span!("TocString::from_scsi_readtoc_0010b", ?bytes).entered();

        let (entries, rem) = bytes.as_chunks::<11>();
        rem.is_empty()
            .ok_or_else(|| {
                io::Error::new(
                    ErrorKind::InvalidData,
                    "invalid 0010b output: not multiple of 11 bytes",
                )
            })
            .or_warn("")?;
        let descriptors = entries.to_vec();
        Ok(Self(descriptors))
    }
}

impl TryFrom<RawTocDescriptors> for TocEntries {
    type Error = io::Error;

    fn try_from(descriptors: RawTocDescriptors) -> Result<Self, Self::Error> {
        descriptors
            .0
            .into_iter()
            .map(TrackDescriptor::from_bytes)
            .map(Option::<TocEntry>::try_from)
            .filter_map(Result::transpose)
            .try_collect()?
    }
}
