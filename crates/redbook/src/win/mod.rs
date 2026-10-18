//! Safe and sane wrappers around Windows APIs for CD drive access

pub use audiocd::AudioCd;

#[forbid(unsafe_code)]
mod audiocd;

/// Windows bindings generated & validated up-to-date via tests/win_bindings.rs
mod bindings;

#[forbid(unsafe_code)]
pub mod cda;

#[forbid(unsafe_code)]
pub mod convert;

#[forbid(unsafe_code)]
pub mod drive;

/// Max size of a windows dos-compatible path (260 ASCII chars).
///
/// See https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
///
/// This has space for "\\.\{260 chars}\0"
///
/// # Note
/// - This is the number of chars, not bytes!
const MAX_PATH_CHARS: usize = 265;
