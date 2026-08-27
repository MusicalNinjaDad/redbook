# Redbook

CDDA CD digital audio as per RedBook (IEC 60908:1999) in rust.

> tldr; A load of glue for working with audio CDs + some windows apps

## Library

Most of what this library provides is "glue", bringing together crates which cover different parts of the CD audio landscape into a single, coherent whole.

### End-to-end functionality

1. **Hardware access** Read audio data from a CD
2. **Parse & lookup** information on the album *including coverart*, generate tags & embeddable coverart
3. **Encode music** to wav or flac

### Structure

There are 3 key entry points to the crate, one each for hardware, CD structure, and music data.

- `struct AudioCd` & `trait AudioCdExt` - the main entry point for interfacing with hardware
- `struct Disc` - the main entry point for working with the contents of a CD
- `struct RippedTrack` - the main entry point for the actual music of a given track

### The glue

This allows for example a ripper to be implemented by:

- Opening an `AudioCd` for a drive, reading the TOC and storing details of the `Disc` by calling `AudioCd::new(drive)?`
- Updating the `Disc` with titles, artists, track names from MusicBrainz and cover art from the CoverArtArchive by calling: `cd.disc_mut().update_musicbrainz()?` and `cd.disc_mut().update_coverart()?`
- Ripping the tracks from the `AudioCd` with `cd.rip(tracknumber)?`, encoding them to flac (`rippedtrack.to_flac()`), generating the tags (`cd.disc().tag_for(tracknumber)?`) and the embeddable cover art (`cd.disc().cover_art()`).

### Core functionality alternatives

Most of these are the individual crates which are glued together:

- [cdda_reader](https://crates.io/crates/cd-da-reader) - for direct access to data from audio CDs. This one I didn't use: there are no safety comments around the unsafe ffi calls and I soon realised that auditing them would be at least as much work as re-implementing. Then I noticed that there are safer ways to use the windows ffi than those that `cdda_reader` chooses. If you are looking for something to support reading audio CDs in linux or mac then try this - they are both still to-do for `redbook`.
- [cdtoc](https://crates.io/crates/cdtoc) - for parsing a TOC (table of contents) and calculating links/IDs for online directories
- [musicbrainz_rs](https://crates.io/crates/musicbrainz_rs) - for querying MusicBrainz and parsing the results
- [flacenc](https://crates.io/crates/flacenc) - for encoding to FLAC (currently slightly broken after changes to `portable_simd` earlier this year)
- [metaflac](https://crates.io/crates/metaflac) - for tagging FLAC files

### Limitations / TODOs

Currently this library only supports things I want to use personally. That means:

- **Hardware access** is limited to Windows. Linux is todo; Mac is open for contributions.
- **Online services** are limited to MusicBrainz. Other services are open for contributions.
- **Output formats** are limited to wav and flac. Other formats are open for contributions.

## Apps

- rip.exe - rip a CD to flac
- toc.exe - dump the TOC as windows reads it
- tag.exe - read tags from a flac file
