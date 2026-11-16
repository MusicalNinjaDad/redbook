use std::{
    error::Error,
    fmt::{self, Display},
    io,
};

use crate::error::RedbookError;

#[derive(Debug)]
pub enum MetaDataError {
    UnknownMimeType(UnknownMimeType),
    MetaflacError(metaflac::Error),
}

impl Error for MetaDataError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            MetaDataError::UnknownMimeType(error) => Some(error),
            MetaDataError::MetaflacError(error) => Some(error),
        }
    }

    fn provide<'a>(&'a self, request: &mut std::error::Request<'a>) {
        match self {
            MetaDataError::UnknownMimeType(_) => {
                request.provide_value(io::ErrorKind::InvalidData);
            }
            MetaDataError::MetaflacError(error) => match &error.kind {
                metaflac::ErrorKind::Io(error) => {
                    request.provide_value(error.kind());
                }
                metaflac::ErrorKind::StringDecoding(_) => {
                    request.provide_value(io::ErrorKind::InvalidData);
                }
                metaflac::ErrorKind::InvalidInput => {
                    request.provide_value(io::ErrorKind::InvalidInput);
                }
            },
        }
    }
}

impl Display for MetaDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetaDataError::UnknownMimeType(_) => write!(f, "unknown MIME type"),
            MetaDataError::MetaflacError(_) => write!(f, "metaflac error"),
        }
    }
}

impl From<UnknownMimeType> for MetaDataError {
    fn from(error: UnknownMimeType) -> Self {
        Self::UnknownMimeType(error)
    }
}

impl From<metaflac::Error> for MetaDataError {
    fn from(error: metaflac::Error) -> Self {
        Self::MetaflacError(error)
    }
}

impl RedbookError for MetaDataError {}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnknownMimeType(Box<str>);

impl Error for UnknownMimeType {
    // no provide - this could be InvalidData or InvalidInput
}

impl RedbookError for UnknownMimeType {}

impl Display for UnknownMimeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for UnknownMimeType {
    fn from(value: &str) -> Self {
        Self(value.into())
    }
}
