#[expect(unsafe_code, reason = "generated bindings to windows API via ffi")]
#[expect(
    clippy::undocumented_unsafe_blocks,
    reason = "generated bindings to windows API via ffi"
)]
#[expect(nonstandard_style)]
#[expect(clippy::upper_case_acronyms)]
#[expect(
    dead_code,
    reason = "until we split and have an extra use for auto-generated stuff that we don't use"
)]
mod bindgen;
mod mocks;

pub(crate) use bindgen::*;
