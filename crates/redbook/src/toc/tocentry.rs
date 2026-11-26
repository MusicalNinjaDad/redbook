//! Provides the types for individual TOC entries

use std::{cmp::Ordering, fmt::Display, num::NonZeroU8};

use derive_more::Display;

use super::{Control, Ctrl, Point, PointKind, TocError, TrackNo};
use crate::{Frame, disc::Track, toc::Msf};

/// A single, classified entry from a CD's Table of Contents (TOC).
///
/// Entries are classified from [`RawTocEntry`]s by their POINT field: a normal track number
/// becomes a [`Track`][TocEntry::Track] entry, [`b0010::FIRST_TRACK_ID`] /
/// [`b0010::LAST_TRACK_ID`] become [`TrackInfo`][TocEntry::TrackInfo] entries, a lead-out point
/// ([`b0000::LEADOUT_TRACK_ID`] or [`b0010::LEADOUT_TRACK_ID`]) becomes a
/// [`Leadout`][TocEntry::Leadout] entry, and anything else is kept as [`Unknown`][TocEntry::Unknown]
/// raw data.
///
/// Classification is validating: `Option::<TocEntry>::from(raw)` returns `None` when an entry is
/// recognizably a track / lead-out / track-info entry but invalid (e.g. an A0/A1 point without
/// session info, or a non-normal track number in PMIN), rather than classifying it as `Unknown`.
///
/// The classification logic is split into [`RawTocEntry::kind`] (which determines the entry
/// kind from ADR/POINT/control) and individual `TryFrom<RawTocEntry>` impls for
/// [`TrackEntry`], [`LeadoutEntry`] and [`InfoEntry`]; `From<RawTocEntry> for Option<TocEntry>`
/// glues these together.
///
/// # Notes
/// - As this library is focussed on audio (redbook), only data relevant to audio tracks is
///   modelled. Use the [`Ctrl`] trait (e.g. [`.control().kind()`][Ctrl::kind]) to distinguish audio
///   from data entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TocEntry {
    /// An audio or data track - data parsed from POINT & PMin,PSec,PFrame
    Track(TrackEntry),
    /// A leadout - based on POINT
    Leadout(LeadoutEntry),
    /// First/Last track number - based on POINT, data parsed from PMin
    TrackInfo(InfoEntry),
    /// Something else - raw data
    Unknown(RawTocEntry),
}

impl From<TrackEntry> for TocEntry {
    fn from(entry: TrackEntry) -> Self {
        Self::Track(entry)
    }
}

impl From<LeadoutEntry> for TocEntry {
    fn from(entry: LeadoutEntry) -> Self {
        Self::Leadout(entry)
    }
}

impl From<InfoEntry> for TocEntry {
    fn from(entry: InfoEntry) -> Self {
        Self::TrackInfo(entry)
    }
}

/// Attempts to classify a raw entry as a [`TrackEntry`].
///
/// Fails when the entry is not a track entry (ADR != 1 or POINT is not a normal track number),
/// or when the payload is invalid. Use [`RawTocEntry::kind`] if you need to check applicability
/// first. Or use `From<RawTocEntry> for Option<TocEntry>` directly to identify, validate and
/// convert in one step.
impl TryFrom<RawTocEntry> for TrackEntry {
    type Error = TocError;

    fn try_from(raw: RawTocEntry) -> Result<Self, Self::Error> {
        match raw.adr() {
            1 => Ok(TrackEntry::new(
                TrackNo::try_from(raw.point)?,
                raw.pmsf().into(),
                raw.control(),
                raw.session(),
            )),
            _ => Err(TocError::InvalidTrackEntry(raw)),
        }
    }
}

/// Attempts to classify a raw entry as a [`LeadoutEntry`].
///
/// Fails when the entry is not a lead-out entry (ADR ≠ 1 or POINT is not a lead-out code).
/// Use [`RawTocEntry::kind`] to check applicability first.
impl TryFrom<RawTocEntry> for LeadoutEntry {
    type Error = TocError;

    fn try_from(raw: RawTocEntry) -> Result<Self, Self::Error> {
        match (raw.adr(), raw.point()) {
            (1, point) if point.is_leadin() => Ok(LeadoutEntry::new(
                raw.pmsf().into(),
                raw.control(),
                raw.session(),
            )),
            _ => Err(TocError::InvalidLeadoutEntry(raw)),
        }
    }
}

/// Attempts to classify a raw entry as an [`InfoEntry`] (first/last track number of a session).
///
/// Fails when the entry is not an info entry (ADR ≠ 1 or POINT is not 0xA0/0xA1), when session
/// info is missing, or when the PMIN payload is not a normal track number.
/// Use [`RawTocEntry::kind`] to check applicability first.
impl TryFrom<RawTocEntry> for InfoEntry {
    type Error = TocError;

    fn try_from(raw: RawTocEntry) -> Result<Self, Self::Error> {
        let info_type = match (raw.adr(), raw.point().info_kind()) {
            (1, Some(info_type)) => info_type,
            _ => return Err(TocError::InvalidInfoEntry(raw)),
        };
        let session = raw.session().ok_or(TocError::InvalidInfoEntry(raw))?;
        let track = TrackNo::new(raw.pmsf().as_u8s().0)?;
        Ok(InfoEntry::new(session, track, raw.control(), info_type))
    }
}

/// Classifies a raw entry into a higher-level [`TocEntry`], returning `None` when the entry is
/// recognizably a track / lead-out / track-info entry but invalid (e.g. an A0/A1 point without
/// session info, or with a non-normal track number in PMIN).
///
/// This is the glue over [`RawTocEntry::kind`] and the individual
/// [`TryFrom<RawTocEntry>`][TrackEntry#impl-TryFrom<RawTocEntry>-for-TrackEntry] impls:
/// `kind()` decides which conversion to attempt, and a failed `TryFrom` becomes `None`.
///
/// [`TocEntry::Unknown`] is reserved for valid entries which the higher-level abstractions don't
/// model (e.g. session boundary or writing-speed details).
impl From<RawTocEntry> for Option<TocEntry> {
    fn from(raw: RawTocEntry) -> Self {
        use TocEntryKind::*;

        match raw.kind() {
            Track => TrackEntry::try_from(raw).ok().map(TocEntry::Track),
            Leadout => LeadoutEntry::try_from(raw).ok().map(TocEntry::Leadout),
            FirstTrackInfo | LastTrackInfo => {
                InfoEntry::try_from(raw).ok().map(TocEntry::TrackInfo)
            }
            Unknown => Some(TocEntry::Unknown(raw)),
            Zero => None,
        }
    }
}

/// Partial ordering *per session* is based upon logical order on the physical disc as follows:
/// - FirstTrackInfo (one per session)
/// - LastTrackInfo (one per session)
/// - Tracks
/// - Leadout (one per session)
/// - Any additional raw entries
/// - ... next session ...
///
/// If entries are mixed from different discs then meaningful ordering may be impossible
impl PartialOrd for TocEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use TocEntry::*;
        use std::cmp::Ordering::*;

        match self.session().cmp(&other.session()) {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => match (self, other) {
                (TrackInfo(i), TrackInfo(o)) => i.partial_cmp(o),
                (TrackInfo(_), _) => Some(Less),
                (_, TrackInfo(_)) => Some(Greater),
                (Track(t), Track(o)) => t.partial_cmp(o),
                (Track(_), _) => Some(Less),
                (_, Track(_)) => Some(Greater),
                (Leadout(l), Leadout(o)) => l.partial_cmp(o),
                (Leadout(_), _) => Some(Less),
                (_, Leadout(_)) => Some(Greater),
                (Unknown(raw), Unknown(o)) => raw.partial_cmp(o),
            },
        }
    }
}

impl Ctrl for TocEntry {
    fn control(&self) -> Control {
        match self {
            TocEntry::Track(entry) => entry.control(),
            TocEntry::Leadout(entry) => entry.control(),
            TocEntry::TrackInfo(entry) => entry.control(),
            TocEntry::Unknown(entry) => entry.control(),
        }
    }
}

impl TocEntry {
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed).
    pub fn session(&self) -> Option<NonZeroU8> {
        match self {
            TocEntry::Track(entry) => entry.session(),
            TocEntry::Leadout(entry) => entry.session(),
            TocEntry::TrackInfo(entry) => Some(entry.session()),
            TocEntry::Unknown(entry) => entry.session(),
        }
    }

    /// The track number this entry relates to, if it has one.
    ///
    /// `Some` for [`Track`][TocEntry::Track] and [`TrackInfo`][TocEntry::TrackInfo] entries;
    /// `None` for [`Leadout`][TocEntry::Leadout] and [`Unknown`][TocEntry::Unknown] entries.
    pub fn trackno(&self) -> Option<TrackNo> {
        match self {
            TocEntry::Track(entry) => Some(entry.trackno()),
            TocEntry::Leadout(_) => None,
            TocEntry::TrackInfo(entry) => Some(entry.trackno()),
            TocEntry::Unknown(_) => None,
        }
    }

    /// Whether this is an audio [`Track`][TocEntry::Track] entry (control code & track number).
    pub fn is_audio_track(&self) -> bool {
        self.is_track() && self.is_audio()
    }

    /// Whether this is a data [`Track`][TocEntry::Track] entry (control code & track number).
    pub fn is_data_track(&self) -> bool {
        self.is_track() && self.is_data()
    }

    /// Whether this is a [`Track`][TocEntry::Track] entry.
    pub fn is_track(&self) -> bool {
        matches!(self, TocEntry::Track(_))
    }

    /// Whether this is a [`Leadout`][TocEntry::Leadout] entry.
    pub fn is_leadout(&self) -> bool {
        matches!(self, TocEntry::Leadout(_))
    }

    /// The [`LeadoutEntry`] payload, if this is a [`Leadout`][TocEntry::Leadout] entry.
    pub fn leadoutentry(&self) -> Option<&LeadoutEntry> {
        match self {
            TocEntry::Leadout(leadout) => Some(leadout),
            _ => None,
        }
    }

    /// The [`TrackEntry`] payload, if this is a [`Track`][TocEntry::Track] entry.
    pub fn trackentry(&self) -> Option<&TrackEntry> {
        match self {
            TocEntry::Track(entry) => Some(entry),
            _ => None,
        }
    }

    /// The [`InfoEntry`] payload, if this is a [`TrackInfo`][TocEntry::TrackInfo] entry.
    pub fn infoentry(&self) -> Option<&InfoEntry> {
        match self {
            TocEntry::TrackInfo(entry) => Some(entry),
            _ => None,
        }
    }

    /// The absolute start position on the disc (including the 150-frame lead-in), if this
    /// entry has one: [`Track`][TocEntry::Track] and [`Leadout`][TocEntry::Leadout] entries do.
    pub fn start(&self) -> Option<Frame> {
        match self {
            TocEntry::Track(entry) => Some(entry.start()),
            TocEntry::Leadout(entry) => Some(entry.start()),
            TocEntry::TrackInfo(_) => None,
            TocEntry::Unknown(_) => None,
        }
    }

    /// The kind of entry, combining the variant with the control code (audio vs data).
    ///
    /// Useful for cases where you need to pattern match on the detailed type without
    /// deconstructing the related entry data.
    pub fn kind(&self) -> TocEntryKind {
        use FirstLast::*;

        match self {
            TocEntry::Track(_) => TocEntryKind::Track,
            TocEntry::Leadout(_) => TocEntryKind::Leadout,
            TocEntry::TrackInfo(InfoEntry {
                info_type: First, ..
            }) => TocEntryKind::FirstTrackInfo,
            TocEntry::TrackInfo(InfoEntry {
                info_type: Last, ..
            }) => TocEntryKind::LastTrackInfo,
            TocEntry::Unknown(_) => TocEntryKind::Unknown,
        }
    }
}

// TODO add PartialEq for other entry types
impl PartialEq<TrackEntry> for TocEntry {
    fn eq(&self, track_entry: &TrackEntry) -> bool {
        matches!(self, TocEntry::Track(entry) if entry == track_entry)
    }
}

impl PartialEq<TocEntry> for TrackEntry {
    fn eq(&self, tocentry: &TocEntry) -> bool {
        matches!(tocentry, TocEntry::Track(entry) if entry == self)
    }
}

impl Display for TrackEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{n}, start: {start}, control codes: {ctl}",
            n = self.track,
            start = self.start,
            ctl = self.control
        )
    }
}

impl PartialEq<cdtoc::Track> for TrackEntry {
    fn eq(&self, toc_track: &cdtoc::Track) -> bool {
        (Some(self.track) == TrackNo::checked_new(toc_track.number()))
            && Msf::try_from(toc_track.msf()).is_ok_and(|msf| self.start == msf)
    }
}

/// The kind of TocEntry. Returned by [TocEntry::kind] and used in `Result<TocEntries, TocError>::collect()`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
#[expect(missing_docs, reason = "clear naming")]
pub enum TocEntryKind {
    Track,
    Leadout,
    FirstTrackInfo,
    LastTrackInfo,
    Unknown,
    /// An entry will all zeros
    Zero,
}

/// Entry in a CD TOC (Table of Contents) for an audio track.
///
/// Represents a single entry from the CD's Table of Contents, containing the
/// track number and its absolute start position on the disc.
///
/// # Notes
///
/// - The start position includes the lead-in area (150 frames)
/// - As this library is focussed on audio (redbook), only data relevant to audio tracks is
///   maintained. Conversion functions should be designed discard or fail on data tracks.
///
/// # Examples
///
/// ```
/// use std::num::NonZeroU8;
///
/// use redbook::{Frame, LEADIN};
/// use redbook::toc::{control::Control, tocentry::TrackEntry, TrackNo};
///
/// let entry = TrackEntry::new(
///     TrackNo::strict_new(1),
///     LEADIN,
///     Control::default(),
///     Some(NonZeroU8::MIN),
/// );
/// assert_eq!(entry.trackno(), TrackNo::strict_new(1));
/// assert_eq!(entry.start(), LEADIN);
/// assert!(entry.is_audio_track());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TrackEntry {
    /// The 1-indexed track number.
    track: TrackNo,
    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    start: Frame,
    /// The control code, identifying track type
    control: Control,
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed)
    session: Option<NonZeroU8>,
}

impl TrackEntry {
    /// Construct a new [`TrackEntry`].
    ///
    /// # Notes
    /// - `track` needs no validation: a [`TrackNo`] is always a valid track number (1..=99).
    ///   "Special" POINT values (e.g. 0xA0..=0xA2, 0xAA) are [`Point`]s and belong in the other
    ///   [`TocEntry`] variants.
    /// - `start` is deliberately *not* validated against the lead-in: copy-protected discs
    ///   (e.g. Cactus Data Shield, Key2Audio) deliberately store non-standard track positions,
    ///   and input from any reasonable source must be handled without failing.
    /// - session validity is enforced at compile time: `session` is an `Option<NonZeroU8>`
    pub fn new(track: TrackNo, start: Frame, control: Control, session: Option<NonZeroU8>) -> Self {
        Self {
            track,
            start,
            control,
            session,
        }
    }

    /// Absolute start position of this track on the disc, including lead-in (150 frames).
    pub fn start(&self) -> Frame {
        self.start
    }

    /// The 1-indexed track number.
    pub fn trackno(&self) -> TrackNo {
        self.track
    }

    /// The session this entry relates to, if known.
    pub fn session(&self) -> Option<NonZeroU8> {
        self.session
    }

    /// Is an audio track (based on control code & track number)
    pub fn is_audio_track(&self) -> bool {
        self.is_audio()
    }

    /// Is an data track (based on control code & track number)
    pub fn is_data_track(&self) -> bool {
        self.is_data()
    }
}

impl Ctrl for TrackEntry {
    fn control(&self) -> Control {
        self.control
    }
}

impl PartialOrd for TrackEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use Ordering::*;

        (self.session.is_some() == other.session.is_some()).then_some(())?;

        match self.session.cmp(&other.session) {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ if self.track.cmp(&other.track) == self.start.cmp(&other.start) => {
                Some(self.track.cmp(&other.track))
            }
            _ => None,
        }
    }
}

impl PartialEq<Track<'_>> for TrackEntry {
    fn eq(&self, track: &Track<'_>) -> bool {
        self.is_audio_track() && (self.track == track.trackno()) && (self.start == track.start())
    }
}

/// Entry in a CD TOC (Table of Contents) for the lead-out.
///
/// The lead-out marks the end of a session. It has no track number, only an absolute start
/// position on the disc (including the 150-frame lead-in).
///
/// # Notes
/// - Construct via [`LeadoutEntry::new`]. The start position is deliberately *not* validated
///   against the lead-in: copy-protected discs store non-standard positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct LeadoutEntry {
    /// Absolute start position of the lead-out on the disc, including lead-in (150 frames).
    start: Frame,
    /// The control code, identifying track type
    control: Control,
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed)
    session: Option<NonZeroU8>,
}

impl LeadoutEntry {
    /// Construct a new [`LeadoutEntry`].
    ///
    /// # Notes
    /// - `start` is deliberately *not* validated against the lead-in, for the same reason as
    ///   [`TrackEntry::new`]: copy-protected discs store non-standard positions.
    /// - session validity is enforced at compile time: `session` is an `Option<NonZeroU8>`.
    pub fn new(start: Frame, control: Control, session: Option<NonZeroU8>) -> Self {
        Self {
            start,
            control,
            session,
        }
    }

    /// Absolute start position of the lead-out on the disc, including lead-in (150 frames).
    pub fn start(&self) -> Frame {
        self.start
    }

    /// The session this entry relates to, if known.
    pub fn session(&self) -> Option<NonZeroU8> {
        self.session
    }
}

/// Partial Ordering based on session then start. If these match but control differs then
/// no meaningful ordering is possible
impl PartialOrd for LeadoutEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use Ordering::*;

        (self.session.is_some() == other.session.is_some()).then_some(())?;

        match self
            .session
            .cmp(&other.session)
            .then_with(|| self.start.cmp(&other.start))
        {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => None,
        }
    }
}

impl Ctrl for LeadoutEntry {
    fn control(&self) -> Control {
        self.control
    }
}

/// Entry in a CD TOC (Table of Contents) holding session info: the first or last track number
/// of a session.
///
/// Encoded as POINT 0xA0 ([`b0010::FIRST_TRACK_ID`]) / 0xA1 ([`b0010::LAST_TRACK_ID`]) in READ
/// TOC format 0b0010, with the track number stored in the start minute (PMIN) of the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InfoEntry {
    /// The session this entry relates to (`NonZeroU8`: session numbers are 1-indexed).
    session: NonZeroU8,
    /// The first or last track number of the session, as encoded in the start minute.
    track: TrackNo,
    /// The control code, identifying track type
    control: Control,
    /// Whether this holds the first or the last track number of the session.
    info_type: FirstLast,
}

impl InfoEntry {
    /// Construct a new [`InfoEntry`] (first/last track number info).
    ///
    /// # Notes
    /// - `track` needs no validation: a [`TrackNo`] is always a valid track number (1..=99)
    /// - session validity is enforced at compile time: `session` is a [`NonZeroU8`]
    pub fn new(session: NonZeroU8, track: TrackNo, control: Control, info_type: FirstLast) -> Self {
        Self {
            session,
            track,
            control,
            info_type,
        }
    }

    /// The session this entry relates to.
    pub fn session(&self) -> NonZeroU8 {
        self.session
    }

    /// The first or last track number of the session.
    pub fn trackno(&self) -> TrackNo {
        self.track
    }

    /// Whether this holds the first or the last track number of the session.
    pub fn info_type(&self) -> FirstLast {
        self.info_type
    }
}

impl Ctrl for InfoEntry {
    fn control(&self) -> Control {
        self.control
    }
}

/// Whether an [`InfoEntry`] holds the first or the last track number of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FirstLast {
    /// The first track number of the session (POINT [`b0010::FIRST_TRACK_ID`])
    First,
    /// The last track number of the session (POINT [`b0010::LAST_TRACK_ID`])
    Last,
}

/// Partial ordering based on session then type. If these match but other fields differ then
/// no meaningful ordering is possible
impl PartialOrd for InfoEntry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use Ordering::*;

        match self
            .session
            .cmp(&other.session)
            .then_with(|| self.info_type.cmp(&other.info_type))
        {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => None,
        }
    }
}

/// A raw, not-yet-classified TOC entry: the fields of a Q sub-channel / READ TOC track
/// descriptor, exactly as read from the disc.
///
/// Classify into a higher-level [`TocEntry`] via `Option::<TocEntry>::from(raw)` — `None` means
/// the entry is recognizably a track / lead-out / track-info entry but invalid.
///
/// For finer-grained control use [`RawTocEntry::kind`] to determine the entry kind, then the
/// individual `TryFrom<RawTocEntry>` impls ([`TrackEntry`], [`LeadoutEntry`], [`InfoEntry`]) to
/// extract the classified payload.
///
/// The relevant definitions are:
///
/// #### READ_TOC format 0010b (detailled TOC, multi-session)
///
/// CTRL   ADR | TNO | POINT | MIN | SEC | FRAME | ZERO | PMIN | PSEC | PFRAME
/// -- | -- | -- | -- | -- | -- | -- | -- | -- | -- | --
/// 4 or 6 | 1 | 00h | 01h–63h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Start position of track | Start position of track | Start position of track
/// 4 or 6 | 1 | 00h | A0h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | First Track Number | Disc Type | 00h
/// 4 or 6 | 1 | 00h | A1h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Last Track Number | 00h | 00h
/// 4 or 6 | 1 | 00h | A2h | ATIME (Absolute time) | ATIME (Absolute time) | ATIME (Absolute time) | 00h | Start position of Lead-out | Start position of Lead-out | Start position of Lead-out
/// 4 or 6 | 5 | 00h | B0h | Start time of next possible program in the Recordable Area of the disc | Start time of next possible program in the Recordable Area of the disc | Start time of next possible program in the Recordable Area of the disc | # of pointers in Mode 5 | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc | Maximum start time of outer-most Lead-out area in the Recordable Area of the disc
/// 4 or 6 | 5 | 00h | B1h | 00h | 00h | 00h | 00h | # of skip interval Pointers (N≤40) | # of skip Track Pointers (N≤21) | 00h
/// 4 or 6 | 5 | 00h | B2h–B4h | Skip # | Skip # | Skip # | Skip # | Skip # | Skip # | Skip #
/// 4 or 6 | 5 | 00h | 01h–40h | Ending time for the interval that should be skipped | Ending time for the interval that should be skipped | Ending time for the interval that should be skipped | Reserved | Start time for interval that should be skipped on playback | Start time for interval that should be skipped on playback | Start time for interval that should be skipped on playback
/// 4 or 6 | 5 | 00h | C0h | Optimum recording power | Reserved | Reserved | Reserved | Start time of the first Lead-in Area of the disc | Start time of the first Lead-in Area of the disc | Start time of the first Lead-in Area of the disc
/// 4 or 6 | 5 | 00h | C1h | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP | Copy of information from A1 point in ATIP
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RawTocEntry {
    /// The session this entry relates to, if known (`NonZeroU8`: session numbers are 1-indexed)
    session: Option<NonZeroU8>,
    /// The ADR (address mode) field of the Q sub-channel.
    adr: u8,
    /// The control code, identifying track type
    control: Control,
    /// The track number this entry relates to (TNO field).
    tno: u8,
    /// The POINT field: a track number or a special code (e.g. 0xA0..=0xA2, 0xAA).
    point: Point,
    /// The MSF (min, sec, frame) field.
    msf: Msf,
    /// The "zero" byte of the track descriptor.
    zero: u8,
    /// The PMSF (point min, sec, frame) field: the payload data for POINT.
    pmsf: Msf,
}

impl RawTocEntry {
    /// Construct a new [`RawTocEntry`] from raw Q sub-channel / READ TOC data.
    ///
    /// As this is a raw, not-yet-classified entry there is nothing to validate; session validity
    /// is enforced at compile time via `Option<NonZeroU8>`.
    #[expect(
        clippy::too_many_arguments,
        reason = "arguments mirror the fields of a raw READ TOC track descriptor one-to-one"
    )]
    pub fn new(
        session: Option<NonZeroU8>,
        adr: u8,
        control: Control,
        tno: u8,
        point: Point,
        msf: Msf,
        zero: u8,
        pmsf: Msf,
    ) -> Self {
        Self {
            session,
            adr,
            control,
            tno,
            point,
            msf,
            zero,
            pmsf,
        }
    }

    /// The session this entry relates to, if known.
    pub fn session(&self) -> Option<NonZeroU8> {
        self.session
    }

    /// The ADR (address mode) field of the Q sub-channel.
    pub fn adr(&self) -> u8 {
        self.adr
    }

    /// The track number this entry relates to (TNO field).
    pub fn tno(&self) -> u8 {
        self.tno
    }

    /// The POINT field: a track number or a special code (e.g. 0xA0..=0xA2, 0xAA).
    pub fn point(&self) -> Point {
        self.point
    }

    /// The MSF (min, sec, frame) field.
    pub fn msf(&self) -> Msf {
        self.msf
    }

    /// The "zero" byte of the track descriptor.
    pub fn zero(&self) -> u8 {
        self.zero
    }

    /// The PMSF (point min, sec, frame) field: the payload data for POINT.
    pub fn pmsf(&self) -> Msf {
        self.pmsf
    }

    /// Classify this raw entry by its ADR / POINT fields (and control code for the audio/data
    /// distinction), without validating the payload data.
    ///
    /// This is the same classification that [`From<RawTocEntry> for Option<TocEntry>`] applies,
    /// exposed separately so callers can determine which [`TryFrom<RawTocEntry>`] impl to use
    /// before attempting the conversion.
    pub fn kind(&self) -> TocEntryKind {
        use TocEntryKind::*;

        match (self.adr, self.point.kind()) {
            (1, PointKind::LeadIn) => Leadout,
            (1, PointKind::FirstTrackInfo) => FirstTrackInfo,
            (1, PointKind::LastTrackInfo) => LastTrackInfo,
            (1, PointKind::Normal) => Track,
            _ => Unknown,
        }
    }
}

impl Ctrl for RawTocEntry {
    fn control(&self) -> Control {
        self.control
    }
}

impl PartialOrd for RawTocEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        use std::cmp::Ordering::*;

        (self.session.is_some() == other.session.is_some()).then_some(())?;

        match self.session.cmp(&other.session) {
            Less => Some(Less),
            Greater => Some(Greater),
            Equal if self == other => Some(Equal),
            _ => {
                // destructure to ensure we don't forget any fields
                let RawTocEntry {
                    session: _,
                    adr,
                    control,
                    tno,
                    point,
                    msf,
                    zero,
                    pmsf,
                } = self;

                // everything except point must match to be able to order
                (*adr == other.adr
                    && *control == other.control
                    && *tno == other.tno
                    && *msf == other.msf
                    && *zero == other.zero
                    && *pmsf == other.pmsf)
                    .then(|| point.cmp(&other.point))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::trackno::{b0000, b0010};
    use super::*;

    use std::convert::CheckedCastFromInt;

    use rstest::rstest;

    use crate::LEADIN;

    #[test]
    fn track_entry_new_stores_all_fields() {
        let entry = TrackEntry::new(
            TrackNo::strict_new(1),
            LEADIN,
            Control::default(),
            Some(NonZeroU8::MIN),
        );
        assert_eq!(
            (entry.trackno(), entry.start(), entry.session()),
            (TrackNo::strict_new(1), LEADIN, Some(NonZeroU8::MIN))
        );

        // start is deliberately not validated against the lead-in: copy-protected discs
        // (e.g. Cactus Data Shield, Key2Audio) store track starts within the lead-in
        let entry = TrackEntry::new(
            TrackNo::strict_new(1),
            Frame::strict_new(149_u32),
            Control::default(),
            None,
        );
        assert_eq!(entry.start(), Frame::strict_new(149_u32));
    }

    #[test]
    fn leadout_entry_new_stores_all_fields() {
        // start is deliberately not validated against the lead-in (copy-protected discs)
        let entry = LeadoutEntry::new(LEADIN, Control::default(), None);
        assert_eq!(
            (entry.start(), entry.control(), entry.session()),
            (LEADIN, Control::default(), None)
        );
    }

    #[test]
    fn info_entry_new_stores_all_fields() {
        let entry = InfoEntry::new(
            NonZeroU8::MIN,
            TrackNo::strict_new(1),
            Control::default(),
            FirstLast::First,
        );
        assert_eq!(
            (entry.session(), entry.trackno(), entry.info_type()),
            (NonZeroU8::MIN, TrackNo::strict_new(1), FirstLast::First)
        );
    }

    #[test]
    fn raw_toc_entry_new_stores_all_fields() {
        let msf = Msf::new(0, 2, 0);
        let pmsf = Msf::new(1, 2, 3);
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            2,
            Point(3),
            msf,
            0,
            pmsf,
        );
        assert_eq!(
            (
                raw.session(),
                raw.adr(),
                raw.control(),
                raw.tno(),
                raw.point(),
                raw.msf(),
                raw.zero(),
                raw.pmsf()
            ),
            (
                Some(NonZeroU8::MIN),
                1,
                Control::default(),
                2,
                Point(3),
                msf,
                0,
                pmsf
            )
        );
    }

    #[test]
    fn raw_entries_classify_via_validating_constructors() {
        let msf = Msf::new(0, 2, 0x21);
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        let entry = Option::<TocEntry>::from(raw).unwrap();
        assert!(matches!(entry, TocEntry::Track(_)));
        assert_eq!(entry.trackno(), Some(TrackNo::strict_new(1)));
        assert_eq!(entry.start(), Some(Frame::from(msf)));

        // a track start within the lead-in (as stored by copy-protected discs) must still
        // classify as a track rather than failing
        let msf = Msf::new(0, 0, 5);
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Track(_))
        ));
    }

    #[test]
    fn unmodelled_raw_entry_points_become_unknown() {
        let msf = Msf::new(0, 2, 0);
        // a point which is not a normal track number / special code is valid but unmodelled
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(0xB0), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Unknown(_))
        ));
    }

    #[test]
    fn invalid_track_info_entries_are_rejected_not_unknown() {
        let msf = Msf::new(0, 2, 0);
        // A0 point without session info is recognizably a track-info entry, but invalid
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert_eq!(Option::<TocEntry>::from(raw), None);
        // A0 point with a non-normal track number in PMIN is likewise invalid
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(0xAA, 0, 0),
        );
        assert_eq!(Option::<TocEntry>::from(raw), None);
    }

    #[rstest]
    #[case(Point(1), Control::default(), TocEntryKind::Track)]
    #[case(Point(99), Control::default(), TocEntryKind::Track)]
    #[case(
        Point(1),
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::Track
    )]
    #[case(b0000::LEADOUT_TRACK_ID, Control::default(), TocEntryKind::Leadout)]
    #[case(b0010::LEADOUT_TRACK_ID, Control::default(), TocEntryKind::Leadout)]
    #[case(
        b0000::LEADOUT_TRACK_ID,
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::Leadout
    )]
    #[case(
        b0010::FIRST_TRACK_ID,
        Control::default(),
        TocEntryKind::FirstTrackInfo
    )]
    #[case(b0010::LAST_TRACK_ID, Control::default(), TocEntryKind::LastTrackInfo)]
    #[case(
        b0010::FIRST_TRACK_ID,
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::FirstTrackInfo
    )]
    #[case(
        b0010::LAST_TRACK_ID,
        Control::from_bits_truncate(Control::DATA_TRACK.bits()),
        TocEntryKind::LastTrackInfo
    )]
    #[case(Point(0xB0), Control::default(), TocEntryKind::Unknown)]
    // ADR != 1 is always Unknown
    #[case(Point(1), Control::default(), TocEntryKind::Unknown)]
    fn raw_toc_entry_kind_classifies_by_point_and_control(
        #[case] point: Point,
        #[case] control: Control,
        #[case] expected: TocEntryKind,
    ) {
        let msf = Msf::new(0, 2, 0);
        let adr = if expected == TocEntryKind::Unknown && point.is_normal() && point != Point(0xB0)
        {
            0 // force ADR != 1 for the "ADR != 1" case
        } else {
            1
        };
        let raw = RawTocEntry::new(None, adr, control, 0, point, msf, 0, msf);
        assert_eq!(raw.kind(), expected);
    }

    #[test]
    fn try_from_raw_for_track_entry_succeeds_for_normal_point() {
        let msf = Msf::new(0, 2, 0x21);
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(3), msf, 0, msf);
        let entry = TrackEntry::try_from(raw).unwrap();
        assert_eq!(entry.trackno(), TrackNo::strict_new(3));
        assert_eq!(entry.start(), Frame::from(msf));
    }

    #[test]
    fn try_from_raw_for_track_entry_fails_for_non_track_points() {
        let msf = Msf::new(0, 2, 0);
        // leadout point
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0000::LEADOUT_TRACK_ID,
            msf,
            0,
            msf,
        );
        assert!(TrackEntry::try_from(raw).is_err());
        // info point
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert!(TrackEntry::try_from(raw).is_err());
        // ADR != 1
        let raw = RawTocEntry::new(None, 0, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(TrackEntry::try_from(raw).is_err());
    }

    #[test]
    fn try_from_raw_for_leadout_entry_succeeds_for_leadout_points() {
        let msf = Msf::new(0, 2, 0);
        for point in [b0000::LEADOUT_TRACK_ID, b0010::LEADOUT_TRACK_ID] {
            let raw = RawTocEntry::new(None, 1, Control::default(), 0, point, msf, 0, msf);
            let entry = LeadoutEntry::try_from(raw).unwrap();
            assert_eq!(entry.start(), Frame::from(msf));
        }
    }

    #[test]
    fn try_from_raw_for_leadout_entry_fails_for_non_leadout_points() {
        let msf = Msf::new(0, 2, 0);
        // normal track point
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(LeadoutEntry::try_from(raw).is_err());
        // ADR != 1
        let raw = RawTocEntry::new(
            None,
            0,
            Control::default(),
            0,
            b0000::LEADOUT_TRACK_ID,
            msf,
            0,
            msf,
        );
        assert!(LeadoutEntry::try_from(raw).is_err());
    }

    #[test]
    fn try_from_raw_for_info_entry_succeeds_for_valid_a0_a1() {
        let msf = Msf::new(0, 2, 0);
        // FIRST_TRACK_ID with valid session and normal track in PMIN
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        let entry = InfoEntry::try_from(raw).unwrap();
        assert_eq!(entry.info_type(), FirstLast::First);
        assert_eq!(entry.trackno(), TrackNo::strict_new(1));
        assert_eq!(entry.session(), NonZeroU8::MIN);

        // LAST_TRACK_ID
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::LAST_TRACK_ID,
            msf,
            0,
            Msf::new(5, 0, 0),
        );
        let entry = InfoEntry::try_from(raw).unwrap();
        assert_eq!(entry.info_type(), FirstLast::Last);
        assert_eq!(entry.trackno(), TrackNo::strict_new(5));
    }

    #[test]
    fn try_from_raw_for_info_entry_fails_for_invalid() {
        let msf = Msf::new(0, 2, 0);
        // no session
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert!(InfoEntry::try_from(raw).is_err());
        // non-normal track in PMIN
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(0xAA, 0, 0),
        );
        assert!(InfoEntry::try_from(raw).is_err());
        // not an info point
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(InfoEntry::try_from(raw).is_err());
    }

    #[test]
    fn from_raw_glues_kind_and_tryfrom_back_together() {
        let msf = Msf::new(0, 2, 0x21);

        // track
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(1), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Track(_))
        ));

        // leadout (b0000)
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0000::LEADOUT_TRACK_ID,
            msf,
            0,
            msf,
        );
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Leadout(_))
        ));

        // info
        let raw = RawTocEntry::new(
            Some(NonZeroU8::MIN),
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::TrackInfo(_))
        ));

        // unknown
        let raw = RawTocEntry::new(None, 1, Control::default(), 0, Point(0xB0), msf, 0, msf);
        assert!(matches!(
            Option::<TocEntry>::from(raw),
            Some(TocEntry::Unknown(_))
        ));

        // invalid info (no session) → None
        let raw = RawTocEntry::new(
            None,
            1,
            Control::default(),
            0,
            b0010::FIRST_TRACK_ID,
            msf,
            0,
            Msf::new(1, 0, 0),
        );
        assert_eq!(Option::<TocEntry>::from(raw), None);
    }

    #[rstest]
    #[case(0_u8, None)]
    #[case(1_u8, Some(TrackNo::strict_new(1)))]
    #[case(42_u8, Some(TrackNo::strict_new(42)))]
    #[case(99_u8, Some(TrackNo::MAX))]
    #[case(100_u8, None)]
    #[case(0xAA_u8, None)]
    // wider integers are accepted via checked cast, rejected if they do not fit in a u8
    #[case(1_u16, Some(TrackNo::strict_new(1)))]
    #[case(99_u32, Some(TrackNo::strict_new(99)))]
    #[case(100_u16, None)]
    #[case(256_u16, None)]
    #[case(u16::MAX, None)]
    #[case(-1_i32, None)]
    fn new_only_accepts_valid_track_numbers<T>(#[case] value: T, #[case] expected: Option<TrackNo>)
    where
        u8: CheckedCastFromInt<T>,
    {
        assert_eq!(TrackNo::checked_new(value), expected);
    }

    #[test]
    fn strict_and_saturating_new_behave() {
        assert_eq!(TrackNo::strict_new(99_u8), TrackNo::MAX);
        assert_eq!(TrackNo::saturating_new(0_u8), TrackNo::strict_new(1));
        assert_eq!(TrackNo::saturating_new(100_u8), TrackNo::MAX);
        assert_eq!(TrackNo::saturating_new(256_u16), TrackNo::MAX);
        assert_eq!(TrackNo::saturating_new(-1_i32), TrackNo::strict_new(1));
    }
}
