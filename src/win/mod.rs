#![cfg_attr(
    not(target_family = "windows"),
    expect(unused_imports, reason = "stubs")
)]

//! Safe and sane wrappers around Windows APIs for CD drive access

#[forbid(unsafe_code)]
mod audiocd;

// This is where any unsafe ffi usage belongs
pub mod drive;
pub mod toc;

pub use audiocd::{AudioCd, ReadOnlyAudioCd};
