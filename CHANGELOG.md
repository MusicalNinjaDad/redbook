# redbook changelog

## [WIP]

### New features

- structured error hierarchy (issues #165, #190, #192-#195, #197, #198): crate-root `redbook::Error` is a plain enum with a variant per domain level-2 error (`DeviceError`, `DriveError`, `TocError`, `DiscError`, `EncodingError`) plus `Other` (any other `Error + Send + Sync + 'static`, incl. `io::Error`, boxed) and `ThreadCancellation`; `redbook::Result` aliases it. Nested domain error types live in the per-module `error` submodules (`disc::error::{TaggingError, IoError, TocMismatch, UnknownMimeType}`, `drive::error::{DeviceError, InvalidBufferSize, OsError}`, `toc::error::{InvalidTrackNo, InvalidOrder}`, `track::error::FlacError`, `win::{WinError, CdaError}`) Domain enums carry the data which led to the error in their variants (e.g. `TocError::InvalidTrackEntry(RawTocEntry)`, `TocError::InvalidTrackNo(InvalidTrackNo)`, `DriveError::NotFound(TrackNo)`) - there is no stringly-typed `.context(...)` API; constructors capturing a wrapped value allow closure-free `.map_err(MyError::new)` and `From` conversions make `?` work directly. Leaf structs (`InvalidTrackNo`, `frame::InvalidFrame`, `frame::InvalidByteSize`) are "wide" in the values they accept, capturing the offending value without loss. All error types are hand-rolled, implement `std::error::Error` + `Send + Sync`, and error enums are not `#[non_exhaustive]`. `source()` returns the directly wrapped error (a wrapped error is not also rendered by `Display`; data-carrying variants are rendered by `Display` and return `None` from `source()`). All convert to `io::Error` preserving the status-quo `ErrorKind` (wrapped OS errors keep `raw_os_error` end-to-end; `ERROR_NOT_READY` gets a dedicated `DeviceError::NotReady`), and relevant cases convert to the glued crates' error types (`cdtoc`, `musicbrainz_rs`, `flacenc`, `metaflac`), mainly unwrapping
- validating constructors now return `Result`: `TrackNo::new`, `Frame::new` & `ByteSize::new` return `Result<Self, InvalidTrackNo/InvalidFrame/InvalidByteSize>` (carrying the offending value); the const `Option`-returning variants are now `checked_new` (#195). `FromStr` for `ByteSize` removed - the old impl misreported semantic overflow as a parse error on >64-bit `usize` targets (#186); use the validating `ByteSize::new` -> `Result<_, InvalidByteSize>` instead

- type-safe time & size types: `Frame` arithmetic is now explicit via `strict_*` / `checked_*` / `saturating_*` methods (the operators are deliberately not implemented); add/sub accept `Frame` or `Msf`, mul/div/rem accept a scalar integer
- new `frame::ByteSize` type for sizes in bytes: `Frame::SIZE_IN_BYTES` is a `ByteSize`, `Frame::size()` converts a frame count to bytes, and `ByteSize::frames_*` convert back
- `Frame::new` is now generic over any integer type; `Frame::strict_new` (panicking), `Frame::checked_new` (const, `Option`) & `Frame::saturating_new` added alongside
- `Msf` components are readable via `Msf::as_u8s()`; `From<Frame> for Msf` saturates each component so `Msf -> Frame -> Msf` always round-trips
- `TocEntry` is now an enum (`Track` / `Leadout` / `TrackInfo` / `Unknown`) with the payload structs `TrackEntry`, `LeadoutEntry`, `InfoEntry` & `RawTocEntry` — all with private fields, validating constructors & getters. Classification from a `RawTocEntry` is via `Option<TocEntry>::from(raw)`: invalid track/leadout/info entries yield `None`, unrecognised points stay `Unknown`
- `TrackNo` is now always a valid track number (1..=99): private `NonZeroU8` field (so 0 is unrepresentable and no `MIN` const is needed), `TrackNo::MAX`, and validating `new` (returning `Result<_, InvalidTrackNo>`) / const `checked_new` (`Option`) / panicking `strict_new` / `saturating_new` constructors plus `cast` / `checked_cast` / `saturating_cast` / `strict_cast` conversions \u2014 consistent with `Frame` & `ByteSize` (`Deref` & public field removed)
- new `toc::Point` type for raw POINT field values: keeps the "lazy" API (public field, `Deref`/`DerefMut`, `From<u8>`) and adds `kind() -> PointKind` (`Normal` / `LeadIn` / `FirstTrackInfo` / `LastTrackInfo` / `Unknown`), `is_normal()`, `is_leadin()`, `is_info()` & `info_kind() -> Option<FirstLast>`
- the `b0000` / `b0010` POINT constants (`LEADOUT_TRACK_ID`, `FIRST_TRACK_ID`, `LAST_TRACK_ID`) are now `Point`s, and `RawTocEntry::point` is a `Point`; classification uses `Point::kind()`
- `TocEntryKind` variants renamed (`AudioTrack`, `DataLeadout`, `FirstAudioTrackInfo`, ...); `TocEntryKind::is_info` removed — use `TocEntry::infoentry()`

### Breaking changes

- library signatures return `Result<_, DomainError>` (or `redbook::Result<_>` for cross-domain APIs like `AudioCd::open`/`rip`) instead of `io::Result<_>`; `RippedTrack::to_flac` is now fallible (`Result<MemSink<u8>, EncodingError>`) instead of panicking on encode failure
- NOTE: read_cdda takes start as total offset on disc, read_chunk took track + offset
- `Frame::new` returns `Result<Frame, InvalidFrame>` (fallible) instead of panicking; `FRAME_SIZE: usize` removed in favour of `Frame::SIZE_IN_BYTES: ByteSize`
- `Device::Iter` yields `Result<_, DeviceError>` items and `CdDrives` yields `Result<_, Error>` items instead of silently ending iteration on errors (#50): `None` is reserved for the expected end-of-enumeration (`ERROR_NO_MORE_ITEMS`); no-disc (`ERROR_NOT_READY`) drives are still skipped
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
