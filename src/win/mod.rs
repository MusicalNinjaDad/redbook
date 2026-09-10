#![cfg_attr(not(target_family = "windows"), expect(missing_docs, reason = "stubs"))]

//! Safe and sane wrappers around Windows APIs for CD drive access

#[forbid(unsafe_code)]
mod audiocd;

#[expect(unsafe_code, reason = "generated bindings to windows API via ffi")]
#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "generated bindings to windows API via ffi"
)]
#[expect(dead_code)]
#[expect(nonstandard_style)]
#[expect(clippy::upper_case_acronyms)]
/// Windows bindings generated & validated up-to-date via tests/win_bindings.rs
mod bindings;

#[forbid(unsafe_code)]
mod convert;

// These modules are where any unsafe ffi usage occurs
#[cfg(target_family = "windows")]
pub mod drive;
pub mod toc;

pub use audiocd::{AudioCd, ReadOnlyAudioCd};
