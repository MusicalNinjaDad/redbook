//! Level-2 error for the [`win`] module (Windows hardware access): the [`WinError`] struct, plus
//! [`CdaError`] for `.cda` file parsing.
//!
//! Follows the crate-wide conventions documented in [`crate::error`]. In addition, OS errors get
//! dedicated handling via [`OsError`]: the wrapped [`io::Error`] from the OS is available
//! directly (preserving `raw_os_error`), and `ERROR_NOT_READY` (21, no disc in drive) is
//! surfaced as [`DeviceError::NotReady`].
use std::{
    error::Error as StdError,
    fmt::{self, Display},
    io,
};

use crate::drive::error::{DeviceError, OsError};

use super::ERROR_NOT_READY;

/// Error returned by [`win`] APIs. This is an OS-coloured type-wrapper around `io::Error`
/// to allow for OS-specific parsing of error codes such as [`ERROR_NOT_READY`]
#[derive(Debug)]
pub struct WinError(io::Error);

impl StdError for WinError {
    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        if request.would_be_satisfied_by_value_of::<io::Error>() {
            request.provide_value_with(|| self.to_io_error());
        } else if request.would_be_satisfied_by_ref_of::<io::Error>() {
            request.provide_ref(self.as_io_error());
        }
    }
}

impl Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, f)
    }
}

impl OsError for WinError {
    fn as_io_error(&self) -> &io::Error {
        &self.0
    }

    fn into_io_error(self) -> io::Error {
        self.0
    }
}

impl From<io::Error> for WinError {
    fn from(error: io::Error) -> Self {
        Self(error)
    }
}

impl From<WinError> for DeviceError {
    fn from(error: WinError) -> Self {
        match error.error_code() {
            ERROR_NOT_READY => DeviceError::NotReady(error.into_io_error()),
            _ => DeviceError::Other(error.into_io_error()),
        }
    }
}
