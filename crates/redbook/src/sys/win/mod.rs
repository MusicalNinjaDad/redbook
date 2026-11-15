//! Safe and sane wrappers around Windows APIs for CD drive access

#[doc(inline)]
pub use handles::DriveHandle;

mod bindings;

#[expect(
    unsafe_code,
    reason = "conversion to/from raw bytes for test asset dump/load"
)]
mod cdrom_toc;

#[forbid(unsafe_code)]
mod convert;

#[forbid(unsafe_code)]
mod drive;

#[forbid(unsafe_code)]
mod error;

#[expect(unsafe_code, reason = "low level device implementation via ffi")]
mod handles;

#[forbid(unsafe_code)]
mod test_fixtures;
