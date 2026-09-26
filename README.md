# Redbook

CDDA CD digital audio as per RedBook (IEC 60908:1999) in rust.

> tldr; A load of glue for working with audio CDs + some windows apps

## Library

Most of what this library provides is "glue", bringing together crates which cover different parts of the CD audio landscape into a single, coherent whole.

### End-to-end functionality

1. **Hardware access** Read audio data from a CD
2. **Parse & lookup** information on the album, generate tags & embeddable coverart
3. **Encode music** to wav or flac

### Structure

There are 3 key entry points to the crate, one each for hardware, CD structure, and music data.

- `struct AudioCd` & `trait AudioCdExt` - the main entry point for interfacing with hardware
- `struct Disc` - the main entry point for working with the contents of a CD
- `struct RippedTrack` - the main entry point for the actual music of a given track

### Example

```rust
// Open a handle to the drive and read table of contents from the CD
let mut cd: AudioCd = AudioCd::new(drive_path)?;

// Try to get data on this cd from musicbrainz. Continue on (network) errors.
let _ = cd.disc_mut().update_musicbrainz();

// There are often multiple releases with the same tracks - select the right one.
cd.disc_mut().set_release(2);

// Try to get the cover art from CoverArtArchive based on the musicbrainz info.
let _ = cd.disc_mut().update_cover_art();

// Make the AudioCd immutable, so we can safely spawn separate threads to rip & encode data.
let cd = cd.lock();

// rip the first track
let track1 = cd.rip(1)?;

// encode the first track to flac
let track1_flac = track1.to_flac();

// get the tags & embeddable cover art
let tags: Option<VorbisComment> = disc.tag_for(1);
let cover: Option<&Picture> = disc.cover_art();
```

### Core functionality alternatives

Most of these are the individual crates which are glued together:

- [cdda_reader](https://crates.io/crates/cd-da-reader) - for direct access to data from audio CDs. This one I didn't use: there are no safety comments around the unsafe ffi calls and I soon realised that auditing them would be at least as much work as re-implementing. Then I noticed that there are safer ways to use the windows ffi than those that `cdda_reader` chooses. If you are looking for something to support reading audio CDs in linux or mac then try this - they are both still to-do for `redbook`.
- [cdtoc](https://crates.io/crates/cdtoc) - for parsing a TOC (table of contents) and calculating links/IDs for online directories
- [musicbrainz_rs](https://crates.io/crates/musicbrainz_rs) - for querying MusicBrainz and parsing the results
- [flacenc](https://crates.io/crates/flacenc) - for encoding to FLAC (currently slightly broken after changes to `portable_simd` earlier this year)
- [metaflac](https://crates.io/crates/metaflac) - for tagging FLAC files

### Tracing

Redbook leverages [tracing](https://crates.io/crates/tracing). Info, Warn & Error messages
are designed to be directly usable as output from a CLI binary.

### Safety

- Unsafe code is limited to specific hardware access modules.
- `#![deny(unsafe_code)]` with a wide selection of additional lints defined in `Cargo.toml`
- All other modules are marked `#[forbid(unsafe_code)]`.
- Every unsafe call is annotated with `#[expect(unsafe_code, reason = "...")]`.
- All unsafe code includes full safety comments.
- All ffi calls are also mocked with full safety instructions, ensuring that IDE integration
  provides these details in-situ. The correctness of the mock signatures is validated on every
  test run.
- We use miri to check for potential UB (thanks to the mocks we can create tests for miri to run
  that validate all the unsafe callsites)
- It goes without saying but, ALL unsafe code is *hand crafted by humans*. Agents.md
  specifically forbids any unsafe code changes or generation.

### Thread Safety

File handles are not `Sync`, but you almost certainly will want to split reading data from a CD
and processing that data into separate threads. To facilitate this `AudioCdExt` and
`AudioCdExtMut` are separate traits. See the example for how to take advantage to initially
update and mutate metadata, before obtaining calling `lock` and spawning
threads to use that data.

### Nightly only

This crate is nightly only for a few reasons:

- I want to rely on downstream crates which use nightly features, in particular: leveraging
  `poratble_simd` in [`flacenc`](https://crates.io/crates/flacenc) (currently disabled); and `try_trait_v2` for [`exit_safely`](https://crates.io/crates/exit_safely) in
  binaries & tracing ergonomics via [`tracing_result`](https://crates.io/crates/tracing_result).
- I find many of the ergonomic benefits worth the toolchain restriction.
- I want to support development of the language and stabilisation of new features.
The crate uses [`build_safely`](https://crates.io/crates/build_safely) to ensure that every experimental feature behaves as expected and
to avoid future lint errors for stable features

### Limitations / TODOs

Currently this library only supports things I want to use personally. That means:

- **Hardware access** is limited to Windows. Linux is todo; Mac is open for contributions.
- **Online services** are limited to MusicBrainz. Other services are open for contributions.
- **Output formats** are limited to wav and flac. Other formats are open for contributions.

## Apps

- rip.exe     - rip a CD to flac
- rip_cli.exe - pure cli ripper
- toc.exe     - dump the TOC as windows reads it
- tag.exe     - read tags from a flac file

GUI apps are made with [![Made with Slint](crates/redbook-rip/assets/MadeWithSlint-logo-whitebg.png)](https://slint.dev/)
