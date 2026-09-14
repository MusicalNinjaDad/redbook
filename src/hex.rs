//! Hex parsing utilities for CD TOC data
//!
//! # Note
//!
//! This module is not part of the public API and subject to change without the usual
//! semver guarantees.
use std::{
    error::Error,
    fmt::{Debug, Display},
    num::ParseIntError,
};

use crate::hex::HexErrorKind::{InvalidValue, NotPairs};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseHexError {
    kind: HexErrorKind,
}

impl ParseHexError {
    /// Outputs the detailed cause of parsing an integer failing.
    pub const fn kind(&self) -> &HexErrorKind {
        &self.kind
    }
}

impl Error for ParseHexError {}

impl Display for ParseHexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{kind}", kind = self.kind())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HexErrorKind {
    /// A [`ParseIntError`] occured while parsing
    InvalidValue(ParseIntError),
    /// Does not contain a series of valid digit pairs
    NotPairs,
}

impl Display for HexErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InvalidValue(parse_int_error) => write!(f, "{parse_int_error}"),
            NotPairs => write!(f, "does not consist of exact pairs of hex digits"),
        }
    }
}

impl From<ParseIntError> for ParseHexError {
    fn from(error: ParseIntError) -> Self {
        Self {
            kind: InvalidValue(error),
        }
    }
}

/// Convert hex in form `00 01 02` to bytes
#[tracing::instrument(level = "trace", skip(hex), fields(len = hex.len()))]
pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, ParseHexError> {
    // TODO Error handling
    let values = hex
        .chars()
        // alphanumeric to allow for ParseIntError InvalidDigit
        .filter(|c| c.is_ascii_alphanumeric())
        .array_chunks::<2>();
    (values
        .clone()
        .into_remainder()
        .is_empty()
        .ok_or(ParseHexError { kind: NotPairs }))?;
    values
        .map(|ref n| u8::from_str_radix(&n.iter().collect::<String>(), 16))
        .try_collect()
        .map_err(Into::into)
}

/// Convert bytes to a hex dump string in the form `00 01 02 ...`
#[tracing::instrument(level = "trace", skip(bytes), fields(byte_count = bytes.len()))]
pub fn hex_dump(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use std::{assert_matches, num::IntErrorKind};

    use super::*;

    #[test]
    fn hex_dump_empty() {
        assert_eq!(hex_dump(&[]), "");
    }

    #[test]
    fn hex_dump_single_byte() {
        assert_eq!(hex_dump(&[0x00]), "00");
        assert_eq!(hex_dump(&[0xff]), "ff");
    }

    #[test]
    fn hex_dump_multiple_bytes() {
        assert_eq!(hex_dump(&[0x00, 0x01, 0x02]), "00 01 02");
        assert_eq!(hex_dump(&[0x10, 0x20, 0x30, 0x40]), "10 20 30 40");
    }

    #[test]
    fn hex_dump_lowercase() {
        assert_eq!(hex_dump(&[0xab, 0xcd]), "ab cd");
    }

    #[test]
    fn hex_dump_full_range() {
        assert_eq!(hex_dump(&[0x00, 0xff]), "00 ff");
    }

    #[test]
    fn hex_dump_roundtrip() {
        let original: Vec<u8> = vec![0x00, 0x01, 0x02, 0x0a, 0xff];
        let dumped = hex_dump(&original);
        let parsed = hex_to_bytes(&dumped).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn hex_to_bytes_empty() {
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
    }

    #[test]
    fn hex_to_bytes_single() {
        assert_eq!(hex_to_bytes("00").unwrap(), vec![0x00]);
        assert_eq!(hex_to_bytes("ff").unwrap(), vec![0xff]);
    }

    #[test]
    fn hex_to_bytes_multiple() {
        assert_eq!(hex_to_bytes("00 01 02").unwrap(), vec![0x00, 0x01, 0x02]);
        assert_eq!(
            hex_to_bytes("10203040").unwrap(),
            vec![0x10, 0x20, 0x30, 0x40]
        );
    }

    #[test]
    fn hex_to_bytes_with_newlines() {
        assert_eq!(hex_to_bytes("00\n01\n02").unwrap(), vec![0x00, 0x01, 0x02]);
    }

    #[test]
    fn hex_to_bytes_uppercase() {
        assert_eq!(hex_to_bytes("FF").unwrap(), vec![0xff]);
        assert_eq!(hex_to_bytes("AB CD").unwrap(), vec![0xab, 0xcd]);
    }

    #[test]
    fn hex_to_bytes_bad_length() {
        let r = hex_to_bytes("01 3");
        assert_matches!(r, Err(e) if e.kind == HexErrorKind::NotPairs);
    }

    #[test]
    fn hex_to_bytes_bad_char() {
        let r = hex_to_bytes("01 3g ff");
        assert!(r.is_err(), "{r:?}");
        assert_matches!(r,
            Err(e)
            if matches!(e.kind(),
                HexErrorKind::InvalidValue(parse_err)
                if *parse_err.kind() == IntErrorKind::InvalidDigit
            )
        );
    }
}
