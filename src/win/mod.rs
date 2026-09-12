//! Safe and sane wrappers around Windows APIs for CD drive access

mod audiocd;

/// Windows bindings generated & validated up-to-date via tests/win_bindings.rs
mod bindings;

#[forbid(unsafe_code)]
pub mod convert;

// These modules are where any unsafe ffi usage occurs
pub mod drive;
pub mod toc;

pub use audiocd::{AudioCd, ReadOnlyAudioCd};

/// Max size of a windows dos-compatible path (260 ASCII chars).
///
/// See https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
///
/// This has space for "\\.\{260 chars}\0"
///
/// # Note
/// - This is the number of chars, not bytes!
const MAX_PATH_CHARS: usize = 265;
