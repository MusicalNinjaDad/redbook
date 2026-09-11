//! Safe and sane wrappers around Windows APIs for CD drive access

#[forbid(unsafe_code)]
#[cfg(target_family = "windows")]
mod audiocd;

/// Windows bindings generated & validated up-to-date via tests/win_bindings.rs
mod bindings;

#[forbid(unsafe_code)]
pub mod convert;

// These modules are where any unsafe ffi usage occurs
#[cfg(target_family = "windows")]
pub mod drive;
pub mod toc;

#[cfg(target_family = "windows")]
pub use audiocd::{AudioCd, ReadOnlyAudioCd};
