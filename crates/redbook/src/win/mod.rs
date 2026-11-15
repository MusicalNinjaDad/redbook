//! Safe and sane wrappers around Windows APIs for CD drive access

pub use error::WinError;
pub use sys::DriveHandle;

#[forbid(unsafe_code)]
mod convert;

#[forbid(unsafe_code)]
mod drive;

pub mod error;

/// Windows bindings generated & validated up-to-date via tests/win_bindings.rs
mod sys;

/// Test fixtures bridging [`TestAlbum`][crate::test_fixtures::albums::TestAlbum] assets and the
/// Windows `CDROM_TOC` ffi struct.
///
/// Compiled under the same conditions as [`sys::mocks`], which relies on
/// `From<TestAlbum> for CDROM_TOC`.
#[forbid(unsafe_code)]
#[doc(hidden)]
pub mod test_fixtures;

/// Max size of a windows dos-compatible path (260 ASCII chars).
///
/// See https://learn.microsoft.com/en-us/windows/win32/fileio/maximum-file-path-limitation
///
/// This has space for "\\.\{260 chars}\0"
///
/// # Note
/// - This is the number of chars, not bytes!
const MAX_PATH_CHARS: usize = 265;
