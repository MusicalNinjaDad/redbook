//! Test fixtures used within the crate and associated binaries.
//!
//! # Note
//!
//! This module is not part of the public API and subject to change without the usual
//! semver guarantees.

pub mod albums;

use std::path::PathBuf;

/// Load a hex file and parse it as raw bytes
pub fn load_hex_file(path: &PathBuf) -> Vec<u8> {
    let content = std::fs::read_to_string(path).unwrap();
    crate::hex::hex_to_bytes(&content).unwrap()
}
