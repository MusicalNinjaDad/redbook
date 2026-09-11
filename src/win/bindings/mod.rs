#![expect(unsafe_code, reason = "bindings to windows API via ffi")]

#[expect(
    clippy::undocumented_unsafe_blocks,
    nonstandard_style,
    clippy::upper_case_acronyms,
    reason = "generated bindings to windows API via ffi"
)]
#[expect(
    dead_code,
    reason = "until we split and have an extra use for auto-generated stuff that we don't use"
)]
mod bindgen;
mod mocks;

pub(crate) use bindgen::*;
