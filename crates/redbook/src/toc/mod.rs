//! Support for Tables of Contents
//!
//! A [`TocEntries`] collection is built from [`TocEntry`]s. Each entry is classified from a
//! [`RawTocEntry`] (the raw fields of a Q sub-channel / READ TOC track descriptor) into a
//! [`TrackEntry`], [`LeadoutEntry`] or [`InfoEntry`] where the POINT field is recognised, and
//! otherwise kept as [`TocEntry::Unknown`] raw data.

pub use error::TocError;
#[doc(inline)]
pub use frame::{ByteSize, Frame, Msf};
pub use tocentries::TocEntries;
pub use tocentry::TocEntry;
#[doc(inline)]
pub use trackno::{Point, PointKind, TrackNo};

pub mod cda;
pub mod control;
pub mod error;
mod frame;
pub mod tocentries;
pub mod tocentry;
mod trackno;

/// Standard CD lead-in duration in frames.
///
/// According to the RedBook standard, CD audio has a 2-second lead-in area
/// at the beginning of the disc. At 75 frames per second, this equals 150 frames.
///
/// This is used as an offset when calculating absolute frame positions on the disc.
pub const LEADIN: Frame = Frame::strict_new(150);
