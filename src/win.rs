#![expect(missing_docs, reason = "needs update")]
#![cfg_attr(
    not(target_family = "windows"),
    expect(unused_imports, reason = "stubs")
)]

//! Safe and sane wrappers around Windows APIs for CD drive access
//!
//! # Tracing
//!
//! This module emits the following spans:
//! - `CdDrive::open` (INFO): Drive opening with `path` field
//! - `CdDrive::read_chunk` (TRACE): Chunk reading with `track`, `frame_offset`, and `frames_to_read` fields
//! - `AudioCd::new` (INFO): Audio CD initialization with `path` field
//!
//! Events:
//! - `CdDrive::open` warnings on failure with `bytes_read` field
//! - `CdDrive::read_chunk` warnings on failure with `bytes_read` field

// RULES for this file:
// - Use .strict_... for all math functions
// - Panic on failure for any type conversions
// - Use a liberal application of debug_assert

mod audiocd;
pub mod drive;
pub mod toc;

pub use audiocd::{AudioCd, ReadOnlyAudioCd};
