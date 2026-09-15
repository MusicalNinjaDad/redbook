// Unsafe restricted to dedicated wrapper modules
#![deny(unsafe_code)]
// Only applicable to library
#![deny(missing_docs)]
// All experimental features are only enabled when on a toolchain where they are still unstable
#![cfg_attr(unstable_const_ops, feature(const_ops))]
#![cfg_attr(unstable_const_trait_impl, feature(const_trait_impl))]
#![cfg_attr(unstable_default_field_values, feature(default_field_values))]
#![cfg_attr(unstable_exact_size_is_empty, feature(exact_size_is_empty))]
#![cfg_attr(unstable_integer_casts, feature(integer_casts))]
#![cfg_attr(unstable_integer_cast_extras, feature(integer_cast_extras))]
#![cfg_attr(unstable_iter_array_chunks, feature(iter_array_chunks))]
#![cfg_attr(unstable_iter_next_chunk, feature(iter_next_chunk))]
#![cfg_attr(unstable_iterator_try_collect, feature(iterator_try_collect))]
#![cfg_attr(unstable_negative_impls, feature(negative_impls))]
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
//! # Structure
//!
//! There are 3 key entry points to the crate, one each for hardware, CD structure, and music data.
//!
//! - [AudioCd] & [AudioCdExt]  - for interfacing with hardware
//! - [Disc]                    - for working with the contents of a CD
//! - [RippedTrack]             - for the actual music of a given track
//!
//! # Example
//!
//! ```rust, no_run
//! use redbook::{AudioCd, AudioCdExt, AudioCdExtMut};
//! # use std::{io, path::PathBuf};
//! # use metaflac::block::{Picture, VorbisComment};
//! # let drive_path = PathBuf::new();
//!
//! // Open a handle to the drive and read table of contents from the CD
//! let mut cd: AudioCd = AudioCd::new(drive_path)?;
//!
//! // Try to get data on this cd from musicbrainz. Continue on (network) errors.
//! let _ = cd.disc_mut().update_musicbrainz();
//!
//! // There are often multiple releases with the same tracks - select the right one.
//! cd.disc_mut().set_release(Some(2));
//!
//! // Try to get the cover art from CoverArtArchive based on the musicbrainz info.
//! let _ = cd.disc_mut().update_cover_art();
//!
//! // Make the AudioCd immutable, so we can safely spawn separate threads to rip & encode data.
//! let cd = cd.lock();
//!
//! // See bin/rip/main.rs for an example of how to use channels & separate threads to rip & encode.
//!
//! // rip the first track
//! let track1 = cd.rip(1)?;
//!
//! // encode the first track to flac
//! let track1_flac = track1.to_flac();
//!
//! // get the tags & embeddable cover art
//! let tags: Option<VorbisComment> = cd.disc().tag_for(1);
//! let cover: Option<&Picture> = cd.disc().cover_art();
//! # Ok::<(), io::Error>(())
//! ```
//!
//! # Tracing
//!
//! Redbook leverages [tracing](https://crates.io/crates/tracing). Info, Warn & Error messages
//! are designed to be directly usable as output from a CLI binary.
//!
//! # Safety
//!
//! - Unsafe code is limited to specific hardware access modules.
//! - `#![deny(unsafe_code)]` with a wide selection of additional lints defined in `Cargo.toml`
//! - All other modules are marked `#[forbid(unsafe_code)]`.
//! - Every unsafe call is annotated with `#[expect(unsafe_code, reason = "...")]`.
//! - All unsafe code includes full safety comments.
//! - All ffi calls are also mocked with full safety instructions, ensuring that IDE integration
//!   provides these details in-situ. The correctness of the mock signatures is validated on every
//!   test run.
//! - We use miri to check for potential UB (thanks to the mocks we can create tests for miri to run
//!   that validate all the unsafe callsites)
//! - It goes without saying but, ALL unsafe code is *hand crafted by humans*. Agents.md
//!   specifically forbids any unsafe code changes or generation.
//!
//! # Thread Safety
//!
//! File handles are not `Sync`, but you almost certainly will want to split reading data from a CD
//! and processing that data into separate threads. To facilitate this [AudioCdExt] and
//! [AudioCdExtMut] are separate traits. See the example for how to take advantage to initially
//! update and mutate metadata, before obtaining calling [lock][AudioCdExtMut::lock] and spawning
//! threads to use that data.
//!
//! # Nightly only
//!
//! This crate is nightly only for a few reasons:
//! - I want to rely on downstream crates which use nightly features, in particular: leveraging
//!   `poratble_simd` in [`flacenc`] (currently disabled); and `try_trait_v2` for [`exit_safely`] in
//!   binaries & tracing ergonomics via [`tracing_result`].
//! - I find many of the ergonomic benefits worth the toolchain restriction.
//! - I want to support development of the language and stabilisation of new features.
//!
//! The crate uses [`build_safely`] to ensure that every experimental feature behaves as expected and
//! to avoid future lint errors for stable features

#[forbid(unsafe_code)]
mod audiocd;
pub use audiocd::*;

#[forbid(unsafe_code)]
mod disc;
pub use disc::*;

#[forbid(unsafe_code)]
pub mod tagging;

#[forbid(unsafe_code)]
mod track;
pub use track::*;

#[forbid(unsafe_code)]
mod toc;
pub use toc::*;

// provides abstractions over direct hardware access
pub mod win;
#[doc(inline)]
pub use win::AudioCd;

#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod hex;
#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod test_fixtures;

/// Size of a single CDDA audio frame in bytes.
///
/// According to the RedBook standard (IEC 60908:1999), each frame contains 2352 bytes
/// of raw audio data.
pub const FRAME_SIZE: usize = 2352;

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

/// Maximum number of bytes that can be read in a single chunk.
///
/// Calculated as [`MAX_CHUNK_FRAMES`] * [`FRAME_SIZE`].
const MAX_CHUNK_BYTES: usize = MAX_CHUNK_FRAMES * FRAME_SIZE;

/// Standard CD lead-in duration in frames.
///
/// According to the RedBook standard, CD audio has a 2-second lead-in area
/// at the beginning of the disc. At 75 frames per second, this equals 150 frames.
///
/// This is used as an offset when calculating absolute frame positions on the disc.
pub const LEADIN: Frame = Frame::new(150);
