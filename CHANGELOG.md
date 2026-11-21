# redbook changelog

## [WIP]

This release overhauls the public API for future-proof extensibility, usability & type-safety —
all changes below are breaking. They are grouped by theme; the bullets within each group list the
concrete API deltas to aid migration.

### Validated, type-safe value types

`Frame`, `Msf`, `TrackNo` and the new `frame::ByteSize` are now validated newtypes rather than
plain integers or structs with public fields: where the type system can enforce validity it does,
and everything else goes through explicit fallible / checked / saturating constructors &
arithmetic instead of panicking or wrapping silently.

- `TrackNo` is now always a valid track number (1..=99), backed by a private `NonZeroU8`, so `0`
  is unrepresentable and no `MIN` const is needed. Construct via `new` (returns
  `Result<_, InvalidTrackNo>`), const `checked_new` (`Option`), panicking `strict_new` or
  `saturating_new`, and convert via `cast` / `checked_cast` / `saturating_cast` / `strict_cast` —
  the same pattern as `Frame` & `ByteSize`. Removed: tuple construction `TrackNo(n)`, `From<u8>`,
  `FromStr`, `Default`, `Deref`/`DerefMut`, `MIN_NORMAL` / `MAX_NORMAL`, `is_normal` /
  `is_special` / `new_normal`; "special" POINT values are now `Point`s and cannot be `TrackNo`s.
- `Frame::new` is now generic over any integer type and returns `Result<Frame, InvalidFrame>`
  (alongside const `checked_new` -> `Option`, panicking `strict_new` & `saturating_new`);
  `ByteSize::new` likewise returns `Result<_, InvalidByteSize>`. These leaf error types are
  "wide": they capture the offending value without loss, and their constructors allow closure-free
  `.map_err(InvalidFrame::new)`. The old const `Option`-returning constructors are now named
  `checked_new` (#195). `FromStr` for `ByteSize` is removed — it misreported semantic overflow as
  a parse error on >64-bit `usize` targets (#186); use the validating `ByteSize::new` instead.
- Arithmetic on `Frame` & `Msf` is now explicit via `strict_*` (panicking) / `checked_*` (`Option`)
  / `saturating_*` methods — add/sub accept `Frame` or `Msf`, mul/div/rem take a scalar integer —
  the operators themselves are deliberately not implemented. `Msf` fields are private; read them
  via `Msf::as_u8s()`. `From<Frame> for Msf` saturates each component, so `Msf -> Frame -> Msf`
  always round-trips.
- Sizes in bytes are first-class through the new `frame::ByteSize`: `Frame::SIZE_IN_BYTES` is a
  `ByteSize` (replacing the `FRAME_SIZE: usize` const), `Frame::size()` converts a frame count to
  bytes and `ByteSize::frames_*` convert back.

### Enum-first TOC model

`TocEntry` is now an enum (`Track` / `Leadout` / `TrackInfo` / `Unknown`) backed by dedicated
payload structs, replacing the old struct with public `track` / `start` / `control` fields.

- The payload structs `TrackEntry`, `LeadoutEntry`, `InfoEntry` & `RawTocEntry` all have private
  fields, validating constructors & getters. `TrackEntry::new`, `InfoEntry::new` & `Track::new`
  are now infallible — the track-number invariant is upheld by `TrackNo` itself. `Track::toc_entry`
  is a `TrackEntry`: use the `trackno()` / `start()` getters.
- Classify a raw entry with `Option<TocEntry>::from(raw)`: invalid track / leadout / info entries
  yield `None`; unrecognised points stay `Unknown`.
- New `toc::Point` type for raw POINT field values keeps the "lazy" API (public field,
  `Deref`/`DerefMut`, `From<u8>`) and adds classification: `kind() -> PointKind` (`Normal` /
  `LeadIn` / `FirstTrackInfo` / `LastTrackInfo` / `Unknown`), `is_normal()`, `is_leadin()`,
  `is_info()` & `info_kind() -> Option<FirstLast>`. The `LEADOUT_TRACK_ID` / `FIRST_TRACK_ID` /
  `LAST_TRACK_ID` constants are now `Point`s and `RawTocEntry::point` is a `Point`; classification
  uses `Point::kind()`.
- `TocEntryKind` variants renamed (`AudioTrack`, `DataLeadout`, `FirstAudioTrackInfo`, ...);
  `TocEntryKind::is_info` removed — use `TocEntry::infoentry()`; `TrackEntry::kind` removed — use
  `TocEntry::kind()`.

### Structured error hierarchy

Errors are now a typed hierarchy instead of stringly-typed `io::Error`s (issues #165, #190,
#192-#195, #197, #198): crate-root `redbook::Error` is a plain enum with a variant per domain
level-2 error (`DeviceError`, `DriveError`, `TocError`, `DiscError`, `EncodingError`,
`MetaDataError`) plus `Other` (any other `Error + Send + Sync + 'static`, incl. `io::Error`, boxed)
and `ThreadCancellation`; `redbook::Result` aliases it.

- Library signatures return `Result<_, DomainError>` (or `redbook::Result<_>` for cross-domain
  APIs like `AudioCd::open` / `rip`) instead of `io::Result<_>`; `RippedTrack::to_flac` is now
  fallible (`Result<MemSink<u8>, EncodingError>`) instead of panicking on encode failure.
- Domain error types live in the per-module `error` submodules (`disc::error`, `drive::error`,
  `toc::error`) and the crate-root `error` module (`EncodingError` / `FlacError`, `MetaDataError` /
  `UnknownMimeType`); `.cda` parsing has its own `toc::error::CdaError`.
- Variants carry the data which led to the error (e.g. `TocError::InvalidTrackEntry(RawTocEntry)`,
  `TocError::InvalidTrackNo(InvalidTrackNo)`, `DriveError::NotFound(TrackNo)`) — there is no
  stringly-typed `.context(...)` API. Constructors capturing a wrapped value allow closure-free
  `.map_err(MyError::new)`, and `From` conversions make `?` work directly.
- All error types are hand-rolled, implement `std::error::Error + Send + Sync`, and the enums are
  not `#[non_exhaustive]`. `source()` returns the directly wrapped error (a wrapped error is not
  also rendered by `Display`; data-carrying variants are rendered by `Display` and return `None`
  from `source()`).
- All convert to `io::Error` preserving the status-quo `ErrorKind` (wrapped OS errors keep
  `raw_os_error` end-to-end; `ERROR_NOT_READY` gets a dedicated `DeviceError::NotReady`), and
  relevant cases convert to the glued crates' error types (`cdtoc`, `musicbrainz_rs`, `flacenc`,
  `metaflac`), mainly unwrapping.

### Fallible, `TrackNo`-based reading APIs

- `Disc::track`, `Disc::tag_for`, `AudioCd::rip` & `Drive::read_track` take a `TrackNo` instead of
  `usize`; `Disc::track` returns `None` instead of panicking on invalid track numbers.
- `Device::Iter` yields `Result<_, DeviceError>` items and `CdDrives` yields `Result<_, Error>`
  items instead of silently ending iteration on errors (#50): `None` is reserved for the expected
  end-of-enumeration (`ERROR_NO_MORE_ITEMS`); no-disc (`ERROR_NOT_READY`) drives are still skipped.
- Note: `read_cdda` takes `start` as the total offset on the disc (absolute frame position,
  including the lead-in) — the old `read_chunk` took track + offset.
- `RipProgress` fields `bytes_processed` / `total_bytes` replaced by `processed` / `total`, both
  `Frame` counts instead of byte counts.
- `toc::cda::tracks` skips (with a warning) `.cda` files which do not describe a normal audio
  track; `Track`s are built from validated `CdaFile`s.

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
