# redbook changelog

## [WIP]

### New features

- add `VorbisTagExt::directory()` to generate `Artist/Album`-style paths from tags
- `VorbisTagExt::filename()` & `VorbisTagExt::directory()` produce sanitised paths suitable for target OS

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
