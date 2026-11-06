# redbook changelog

## [WIP]

### New features

- type-safe time & size types: `Frame` arithmetic is now explicit via `strict_*` / `checked_*` / `saturating_*` methods (the operators are deliberately not implemented); add/sub accept `Frame` or `Msf`, mul/div/rem accept a scalar integer
- new `frame::ByteSize` type for sizes in bytes: `Frame::SIZE_IN_BYTES` is a `ByteSize`, `Frame::size()` converts a frame count to bytes, and `ByteSize::frames_*` convert back
- `Frame::new` is now generic over any integer type; `Frame::strict_new` (panicking) & `Frame::saturating_new` added alongside
- `Msf` components are readable via `Msf::as_u8s()`; `From<Frame> for Msf` saturates each component so `Msf -> Frame -> Msf` always round-trips
- `TocEntry` is now an enum (`Track` / `Leadout` / `TrackInfo` / `Unknown`) with the payload structs `TrackEntry`, `LeadoutEntry`, `InfoEntry` & `RawTocEntry` — all with private fields, validating constructors & getters. Classification from a `RawTocEntry` is via `Option<TocEntry>::from(raw)`: invalid track/leadout/info entries yield `None`, unrecognised points stay `Unknown`
- `TrackNo` is now always a valid track number (1..=99): private `NonZeroU8` field (so 0 is unrepresentable and no `MIN` const is needed), `TrackNo::MAX`, and validating `new` / panicking `strict_new` / `saturating_new` constructors plus `cast` / `checked_cast` / `saturating_cast` / `strict_cast` conversions — consistent with `Frame` & `ByteSize` (`Deref` & public field removed)
- new `toc::Point` type for raw POINT field values: keeps the "lazy" API (public field, `Deref`/`DerefMut`, `From<u8>`) and adds `kind() -> PointKind` (`Normal` / `LeadIn` / `FirstTrackInfo` / `LastTrackInfo` / `Unknown`), `is_normal()`, `is_leadin()`, `is_info()` & `info_kind() -> Option<FirstLast>`
- the `b0000` / `b0010` POINT constants (`LEADOUT_TRACK_ID`, `FIRST_TRACK_ID`, `LAST_TRACK_ID`) are now `Point`s, and `RawTocEntry::point` is a `Point`; classification uses `Point::kind()`
- `TocEntryKind` variants renamed (`AudioTrack`, `DataLeadout`, `FirstAudioTrackInfo`, ...); `TocEntryKind::is_info` removed — use `TocEntry::infoentry()`

### Breaking changes

- NOTE: read_cdda takes start as total offset on disc, read_chunk took track + offset
- `Frame::new` returns `Option<Frame>` (fallible) instead of panicking; `FRAME_SIZE: usize` removed in favour of `Frame::SIZE_IN_BYTES: ByteSize`
- `Msf` fields are now private; `Frame::new`/`Msf` arithmetic operators removed in favour of the explicit `strict_*` / `checked_*` / `saturating_*` methods
- `TocEntry` struct with public `track`/`start`/`control` fields replaced by the enum-first design described above; `TrackEntry::kind` field removed — use `TocEntry::kind()`
- `TrackNo(n)` tuple construction, `From<u8>`, `FromStr`, `Default`, `Deref`/`DerefMut`, `MIN_NORMAL` / `MAX_NORMAL`, `is_normal` / `is_special` & `new_normal` removed — "special" POINT values are now `Point`s and cannot be `TrackNo`s at all; use `TrackNo::new` / `strict_new`
- `TrackEntry::new`, `InfoEntry::new` & `Track::new` are now infallible (the track-number invariant is upheld by `TrackNo` itself); `Track::toc_entry` is a `TrackEntry` (private fields: use `trackno()` / `start()` getters)
- `Disc::track`, `Disc::tag_for`, `AudioCd::rip` & `Drive::read_track` take a `TrackNo` instead of `usize`; `Disc::track` returns `None` instead of panicking on invalid track numbers
- `RipProgress` fields `bytes_processed` / `total_bytes` replaced by `processed` / `total`, both `Frame` counts instead of byte counts
- `win::cda::tracks` skips (with a warning) `.cda` files which do not describe a normal audio track; `Track` is now built via `TryFrom<CdaFile>`

## [0.4.0]

### New features

- add `VorbisTagExt::directory()` to generate `Artist/Album`-style paths from tags
- `VorbisTagExt::filename()` & `VorbisTagExt::directory()` produce sanitised paths suitable for target OS
- allow for cancellation of long-running IO in multi-threaded applications via `thread_safely::Context`

### Breaking changes

- `Disc::update_thumbnails()` requires a `Context` to be passed

### Bugfixes

- Create output folder if missing (rip.exe)

### Technical

- update deps to `build_safely` v0.6.2 (for `try_blocks`) & `tracing_result` v0.0.2 (for `.ok()`)

## [v0.3.0]

### Breaking changes

Most significant:

- `AudioCd` no longer stores `Disc` in an `Arc`
- `AudioCdExtMut` removed: `disc_mut` is now part of `AudioCdExt`
- `RippedTrack` now includes a full `VorbisComment` along with the raw audio

### New features

- Improved flexibility for downstream users to manage mutli-threading
- GUI app

## [v0.2.1]

### Bugfixes

- fix issue with passing binary artifact between steps for publishing
- use build_safely for feature const_ops

## [v0.2.0]

### New Features

- Identify all available CD-ROM devices
- Double click to run rip.exe

### Technical

- Mock & document ffi
- Test for UB with miri
- Streamline public API
