// Unsafe restricted to dedicated wrapper modules.
// Modules are individually annotated `forbid` or `expect`.
// Many additional safety lints are defined in `Cargo.toml`
#![deny(unsafe_code)]
//
// This lint is only applicable to library, not associated binaries.
#![deny(missing_docs)]
//
// All experimental features are only enabled when on a toolchain where they are still unstable and
// behave as expected.
#![cfg_attr(unstable_const_ops, feature(const_ops))]
#![cfg_attr(unstable_const_trait_impl, feature(const_trait_impl))]
#![cfg_attr(unstable_default_field_values, feature(default_field_values))]
#![cfg_attr(unstable_exact_div, feature(exact_div))]
#![cfg_attr(unstable_exact_size_is_empty, feature(exact_size_is_empty))]
#![cfg_attr(unstable_integer_casts, feature(integer_casts))]
#![cfg_attr(unstable_integer_cast_extras, feature(integer_cast_extras))]
#![cfg_attr(unstable_iter_array_chunks, feature(iter_array_chunks))]
#![cfg_attr(unstable_iter_map_windows, feature(iter_map_windows))]
#![cfg_attr(unstable_iter_next_chunk, feature(iter_next_chunk))]
#![cfg_attr(unstable_iterator_try_collect, feature(iterator_try_collect))]
#![cfg_attr(unstable_os_str_split_at, feature(os_str_split_at))]
#![cfg_attr(unstable_path_absolute_method, feature(path_absolute_method))]
#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]

//! A load of glue for working with CDDA CD digital audio as per RedBook (IEC 60908:1999)
//!
//! Most of what this library provides is "glue", bringing together crates which cover different
//! parts of the CD audio landscape into a single, coherent whole.
//!
//! # End-to-end functionality
//!
//! 1. **Hardware access** Read audio data from a CD
//! 2. **Parse & lookup** information on the album, generate tags  & embeddable coverart
//! 3. **Encode music** to wav or flac
//!
//! # Why?
//!
//! I wanted to create my own cd ripper and found two problems:
//!
//! 1. The only crate available to read data from a CD has lots of undocumented unsafe code and
//!    doesn't make use of any specialised OS or driver functionality, just going straight to hardware.
//!    Auditing was as much work as re-implementing, and re-implementing allowed me to leverage
//!    OS-specific IO-Controls and thereby also make use of any OS and driver functionality.
//! 2. There are a load of great crates out there for each of the other bits of the process, but glueing
//!    them together into a coherent whole needed a lot of code. Those great crates are used here and
//!    worth naming:
//!      - [cdtoc](https://crates.io/crates/cdtoc) - for handling all the online & generic representations of a TOC
//!      - [musicbrainz_rs](https://crates.io/crates/musicbrainz_rs) - for querying MusicBrainz and parsing the results
//!      - [metaflac](https://crates.io/crates/metaflac) - for tagging FLAC files
//!      - [flacenc](https://crates.io/crates/flacenc) - for encoding to FLAC (currently slightly broken after changes to `portable_simd` earlier this year)
//!
//! # Example
//!
//! A simple ripper can be constructed in a few steps. A more full-featured example is included
//! in this crate as the binary `rip-cli` and an even more full-fetured one showing thread-cancellation
//! is available in the separate crate `redbook-rip` in the same repo.
//!
//! ```no_run
//! use std::{fs::{self, File}, io::{self, Write}};
//! use redbook::{AudioCd, CdDrives, RippedTrack, RipProgress, tagging::{PictureExt, VorbisTagExt}};
//! use thread_safely::Controller;
//!
//! // Allow for status updates & cancellation mid-rip
//! let (rip_controller, rip_context) = Controller::<RipProgress>::new();
//!
//! // Find & open the first available CD drive
//! let mut available_drives = CdDrives::all(rip_context)?;
//! let mut first_drive = available_drives.next().ok_or(io::Error::other("No drives found"))?;
//! let mut cd = AudioCd::try_from(first_drive)?;
//!
//! // Get additional meta data from musicbrainz
//! cd.disc_mut().update_musicbrainz()?;
//!
//! // Rip a track
//! let track_1 = cd.rip(1)?;
//!
//! // Encode to flac
//! let flac = track_1.to_flac();
//!
//! // Get associated data
//! let RippedTrack {tags, coverart, ..} = track_1;
//!
//! // Decide where to store the music
//! let album_directory = tags.directory();
//! fs::create_dir_all(&album_directory)?;
//!
//! // Save the flac file
//! let flac_path = album_directory.join(tags.filename()).with_extension("flac");
//! let mut flac_file = File::create_new(&flac_path)?;
//! flac_file.write_all(flac.as_slice())?;
//!
//! // Add tags to the file
//! tags.add_to(&flac_path)?;
//!
//! // Save the coverart as cover.jpg
//! if let Some(artwork) = coverart {
//!     let path = album_directory.join(artwork.filename()?);
//!     fs::write(path, artwork.data)?;
//! }
//! # io::Result::Ok(())
//! ```
//!
//! # Structure
//!
//! ## Main entry point
//!
//! The main entry point is [AudioCd] which provides a single struct representing a CD loaded into
//! a physical drive and allows for:
//!
//! - reading data from the disc
//! - looking up metadata online
//! - creating vorbis tags
//! - encoding music to flac & wav
//!
//! Additionally there are the following lower-level entry points to the crate
//!
//! ## Hardware access
//!
//! In reducing order of abstraction (from highest-level to lowest-level)
//!
//! - [`CdDrives`]  - platform specific iterator over all available physical drives
//! - [`CdDrive`]   - platform specific representation of a physical drive
//! - [`Drive`]     - general trait providing high-level capabilities for a physical drive
//! - [`drive::CdDrives<DEV>`] & [`drive::CdDrive<DEV>`] - general versions of [`CdDrives`] & [`CdDrive`]
//! - [`Device`]    - general trait describing the low-level capabilities of a device able to read CDs
//!
//! ## CD contents
//!
//! In reducing order of abstraction (from highest-level to lowest-level)
//!
//! - [`Disc`]          - a physical CD with its table of contents, tracks, and metadata
//! - [`RippedTrack`]   - the music, coverart and relevant tags for a track
//! - [`Track`]         - a track on a physical CD. This struct cannot stand alone, as it requires
//!   information on relevant [`Disc`] in order to be meaningful.
//! - [`TocEntries`]    - a CD's Table of Contents
//! - [`TocEntry`][toc::TocEntry] - a single entry from a Table of Contents
//! - [`Control`][toc::Control] - technical control code associated with a TOC entry
//! - [`Frame`]         - CD data is split into `Frames` each of which represent 1/75s audio
//! - [`Msf`]           - Minutes, seconds, frames: the time-based unit of measure for audio CDs
//!
//! ## Platform specifics
//!
//! Various safe wrappers around platform specific aspects of audio CD access. See the individual
//! platform module docs for information
//!
//! **Currently windows-only**
//!
//! - [win] - windows-specifics
//!
//! # Technical
//!
//! ## Tracing
//!
//! Redbook leverages [tracing](https://crates.io/crates/tracing). Info, Warn & Error messages
//! are designed to be directly usable as output from a CLI binary. Debug & Trace information is
//! designed to assist with log file analysis.
//!
//! ## Testing
//!
//! The [`test_fixtures`] module provides access to the suite of album data which is used for
//! testing `redbook`.
//!
//! All low-level device access is fully mocked and uses the test data to provide meaningful
//! return values.
//!
//! ## Cross-platform development
//!
//! The top-level struct [`AudioCd`] plus type aliases [`CdDrives`] & [`CdDrive`] are cfg-gated
//! to provide easy access to the the relevant platform-specific implementations.
//!
//! Direct access to all platform-specific variants is available by naming the module path.
//!
//! Your code will always compile: it will use the provided mocks (see: [Testing](#testing)) for
//! foreign platforms and for `#[cfg(test)]` & `#[cfg(doc)]`. That means you don't need to battle
//! with your IDE or rust-analyzer to get full support, but you should take care to add
//! platform-specific cfg-gates if you directly name and use platform-specific code.
//!
//! ## Errors
//!
//! I've made a deliberate choice to use [`io::Error`][std::io::Error] as the error type throughout
//! the library. The reasons for this choice are:
//! - downstream usage is most likely to convert to an io::Error anyway, whether directly or indirectly.
//! - `io::Error` is well known, versatile with meaningful [`ErrorKind`][std::io::ErrorKind]s and
//!   powerful functionality.
//! - `io::Error` is widely understood, a custom error type adds conversion overhead to code and
//!   cognitive overhead for developers
//! - semantically, all errors are related to IO
//!
//! The following `ErrorKind`s are primarily used when constructing and converting new errors:
//! - `ErrorKind::InvalidData` - data conversion errors and un-parsable responses
//! - `ErrorKind::NotFound` - attempts to read a track which does not exist
//! - `ErrorKind::UnexpectedEOF` - data returned is too short to be valid
//! - `ErrorKind::Unsupported` - operations which cannot be performed, e.g. attempting to read
//!   a non-integer number of frames from a disc.
//!
//! A future version *may* introduce a custom error type with a large number of
//! variants and implement `From<...> for io::Error` but at the moment this seems like overhead.
//!
//! # Safety
//!
//! The major driver for directly including hardware access in redbook was the lack of auditable
//! safety considerations in unsafe code in the only alternative available. As such, safety is taken
//! *very seriously* and you can rely on the following to assist with any audits:
//!
//! - Unsafe code is limited to specific hardware access modules.
//! - Every module not requiring unsafe is annotated `#[forbid(unsafe_code)]` as high-up the module
//!   tree as possible.
//! - Every unsafe block is annotated `#[expect(unsafe_code, reason "...")]`
//! - A wide selection of unsafe-specific lints are set to `forbid` in `Cargo.toml`. Lints which
//!   fail for generated bindings are set to `deny`.
//! - All unsafe code includes full safety comments.
//! - All ffi calls are also mocked with full safety instructions, ensuring that IDE integration
//!   provides these details in-situ. The correctness of the mock signatures is validated on every
//!   test run.
//! - We use miri to check for potential UB (thanks to the mocks we can create tests for miri to run
//!   that validate all the unsafe callsites)
//! - It goes without saying but, ALL unsafe code is *hand crafted by humans*. Agents.md
//!   specifically forbids any unsafe code changes or generation.
//!
//! # Multi-threading
//!
//! Realistically any application reading data from a CD will want to run IO in a background thread
//! leaving the main thread available for UI updates etc.
//!
//! [`AudioCd`], [CdDrive] & [CdDrives] require a [`Context::<RipProgress>`][thread_safely::Context]
//! from the [`thread_safely`] crate which provides a means for providing status updates / cancelling
//! long-running IO (e.g. reading an entire track).
//!
//! Great care has been taken to ensure [`Send`] & [`Sync`] are sound, as such multi-threaded
//! applications will need to wrap an [`AudioCd`] in an `Arc<Mutex<AudioCd>>`. This isn't a smell,
//! it's a proper use of those wrappers. A [`Disc`] cannot be [`Sync`] as this would allow one thread
//! to update the meta-data while another was in the process of reading the same meta-data for a given
//! [`Track`].
//!
//! Likewise, [`Clone`] is implemented in a sound manner for low-level [`Device`]s by generating an
//! additional handle to the underlying device rather than simply re-using the existing one.
//!
//! # Nightly only
//!
//! This crate is nightly only for a few reasons:
//! - I want to rely on downstream crates which use nightly features, in particular: leveraging
//!   `portable_simd` in [`flacenc`]; `try_trait_v2` for [`exit_safely`] in binaries & tracing
//!   ergonomics via [`tracing_result`].
//! - I find many of the ergonomic benefits worth the toolchain restriction.
//! - I want to support development of the language and stabilisation of new features.
//!
//! The crate uses [`build_safely`] to ensure that every experimental feature behaves as expected and
//! to avoid future lint errors for stable features

#[doc(inline)]
pub use audiocd::{AudioCd, RipProgress};
#[doc(inline)]
pub use disc::Disc;
#[doc(inline)]
pub use drive::{Device, Drive};
#[doc(inline)]
pub use toc::{Frame, Msf, TocEntries};
pub use track::*;

use crate::win::DriveHandle;

#[forbid(unsafe_code)]
mod audiocd;

#[forbid(unsafe_code)]
pub mod disc;

#[forbid(unsafe_code)]
pub mod drive;

#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod hex;

#[forbid(unsafe_code)]
pub mod tagging;

#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod test_fixtures;

#[forbid(unsafe_code)]
mod toc;

#[forbid(unsafe_code)]
mod track;

// provides abstractions over direct hardware access
pub mod win;

/// Platform specific default CdDrive type
///
/// To define an individually parameterised alternative use `drive::CdDrive<DEV>`
pub type CdDrive = drive::CdDrive<DriveHandle>;

/// Platform specific iterator over the default CdDrive type
///
/// To define an individually parameterised alternative use `drive::CdDrives<DEV>`
pub type CdDrives = drive::CdDrives<DriveHandle>;

/// Size of a single CDDA audio frame in bytes.
///
/// According to the RedBook standard (IEC 60908:1999), each frame contains 2352 bytes
/// of raw audio data.
pub const FRAME_SIZE: usize = 2352;

/// Standard CD lead-in duration in frames.
///
/// According to the RedBook standard, CD audio has a 2-second lead-in area
/// at the beginning of the disc. At 75 frames per second, this equals 150 frames.
///
/// This is used as an offset when calculating absolute frame positions on the disc.
pub const LEADIN: Frame = Frame::new(150);

/// Maximum number of bytes that can be read in a single chunk.
///
/// Calculated as [`MAX_CHUNK_FRAMES`] * [`FRAME_SIZE`].
const MAX_CHUNK_BYTES: usize = MAX_CHUNK_FRAMES * FRAME_SIZE;

/// Maximum number of frames that can be read in a single chunk.
///
/// # Notes
/// - The Windows API's `IOCTL_CDROM_RAW_READ` has an undocumented maximum chunk size
/// - This value is calculated to stay under a safe buffer size (currently 64KB)
///
/// # TODOs
/// - Research the actual maximum chunk size for `IOCTL_CDROM_RAW_READ`
/// - Replace the guessed value (64KB) with a documented reference
const MAX_CHUNK_FRAMES: usize = 64 * 1024 / FRAME_SIZE;
