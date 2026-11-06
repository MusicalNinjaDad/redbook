//! TOC Track descriptor as provided by SCSI READ_TOC command format 0010b.
//!
//! This is the format stored for test assets, which is independently obtained via cdrtools readcd

use std::{
    io::{self, ErrorKind},
    num::NonZeroU8,
};

use tracing_result::Trace;

use crate::{
    TocEntries,
    frame::Msf,
    toc::{Control, RawTocEntry, TocEntry, TrackNo},
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
/// | 2     | TNO (\0) |
/// | 3     | POINT |
/// | 4     | Min (MSF) |
/// | 5     | Sec (MSF) |
/// | 6     | Frame (MSF) |
/// | 7     | Zero |
/// | 8     | PMin |
/// | 9     | PSec |
/// | 10    | PFrame |
///
/// # Note
/// Field naming is based on the Q-Channel definition for the program area. E.g. `TNO` stores the
/// current track number during an audio track, but is `\0` during a LeadIn with the track number
/// for the current TOC entry stored in `POINT`
struct TrackDescriptor {
    /// Session Number (byte 0)
    session: u8,
    /// ADR CONTROL (byte 1): ADR in the high nibble, CONTROL in the low nibble
    adr_control: u8,
    /// TNO (byte 2): 0 (for Q-channel entries *outside* the LeadIn area this stores the track no)
    tno: u8,
    /// POINT field (byte 3): Identfier for the target of this entry (Track details, info, etc.).
    ///
    /// Recognised constants are defined in [`toc::b0010`][crate::toc::b0010]
    point: u8,
    /// Min (byte 4): 0
    min: u8,
    /// Sec (byte 5): 0
    sec: u8,
    /// Frame (byte 6): 0
    frame: u8,
    /// Zero byte (byte 7)
    zero: u8,
    /// PMin: the payload minute for POINT (byte 8)
    pmin: u8,
    /// PSec: the payload second for POINT (byte 9)
    psec: u8,
    /// PFrame: the payload frame for POINT (byte 10)
    pframe: u8,
}

impl TrackDescriptor {
    /// The ADR (address mode) field: the high nibble of `adr_control`
    fn adr(&self) -> u8 {
        (self.adr_control & 0b11110000) >> 4
    }

    /// The CONTROL field: the low nibble of `adr_control`
    fn control(&self) -> Control {
        Control::from_bits_truncate(self.adr_control)
    }

    /// Parse the 11 raw bytes of a single track descriptor
    fn from_bytes(bytes: [u8; 11]) -> Self {
        let mut iter = bytes.iter().copied();

        let session = iter.next().unwrap();
        let adr_control = iter.next().unwrap();
        let tno = iter.next().unwrap();
        let point = iter.next().unwrap();
        let min = iter.next().unwrap();
        let sec = iter.next().unwrap();
        let frame = iter.next().unwrap();
        let zero = iter.next().unwrap();
        let pmin = iter.next().unwrap();
        let psec = iter.next().unwrap();
        let pframe = iter.next().unwrap();

        Self {
            session,
            adr_control,
            tno,
            point,
            min,
            sec,
            frame,
            zero,
            pmin,
            psec,
            pframe,
        }
    }
}

impl TryFrom<TrackDescriptor> for TocEntry {
    type Error = io::Error;

    fn try_from(d: TrackDescriptor) -> Result<Self, Self::Error> {
        let session = NonZeroU8::new(d.session).ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "TOC entry has invalid session number 0",
            )
        })?;
        Option::<TocEntry>::from(RawTocEntry::new(
            Some(session),
            d.adr(),
            d.control(),
            d.tno,
            TrackNo(d.point),
            Msf::new(d.min, d.sec, d.frame),
            d.zero,
            Msf::new(d.pmin, d.psec, d.pframe),
        ))
        .ok_or_else(|| {
            io::Error::new(
                ErrorKind::InvalidData,
                "TOC entry is recognizably a track/leadout/info entry but invalid",
            )
        })
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
            .map(TocEntry::try_from)
            .collect::<io::Result<Vec<TocEntry>>>()?
            .into_iter()
            .collect()
    }
}
