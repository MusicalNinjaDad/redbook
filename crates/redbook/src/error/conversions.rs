use std::io;

use thread_safely::Context;

use crate::{
    Error, RipProgress,
    disc::{DiscError, TocMismatch, error::IoError},
    drive::{
        DriveError,
        error::{DeviceError, InvalidBufferSize},
    },
    error::{EncodingError, FlacError, MetaDataError, UnknownMimeType},
    frame::InvalidByteSize,
    toc::{TocError, error::InvalidOrder},
};

impl From<DeviceError> for Error {
    fn from(error: DeviceError) -> Self {
        Self::DeviceError(error)
    }
}

impl From<InvalidBufferSize> for Error {
    fn from(error: InvalidBufferSize) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<InvalidByteSize> for Error {
    fn from(error: InvalidByteSize) -> Self {
        DeviceError::from(error).into()
    }
}

impl From<DriveError> for Error {
    fn from(error: DriveError) -> Self {
        Self::DriveError(error)
    }
}

impl From<crossbeam::channel::SendError<RipProgress>> for Error {
    fn from(error: crossbeam::channel::SendError<RipProgress>) -> Self {
        DriveError::from(error).into()
    }
}

impl From<TocError> for Error {
    fn from(error: TocError) -> Self {
        Self::TocError(error)
    }
}

impl From<cdtoc::TocError> for Error {
    fn from(error: cdtoc::TocError) -> Self {
        TocError::from(error).into()
    }
}

impl From<InvalidOrder> for Error {
    fn from(error: InvalidOrder) -> Self {
        TocError::from(error).into()
    }
}

impl From<DiscError> for Error {
    fn from(error: DiscError) -> Self {
        Self::DiscError(error)
    }
}

impl From<TocMismatch> for Error {
    fn from(error: TocMismatch) -> Self {
        DiscError::from(error).into()
    }
}

impl From<metaflac::Error> for Error {
    fn from(error: metaflac::Error) -> Self {
        DiscError::from(error).into()
    }
}

impl From<IoError> for Error {
    fn from(error: IoError) -> Self {
        DiscError::from(error).into()
    }
}

impl From<musicbrainz_rs::ApiEndpointError> for Error {
    fn from(error: musicbrainz_rs::ApiEndpointError) -> Self {
        DiscError::from(error).into()
    }
}

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Self {
        DiscError::from(error).into()
    }
}

impl From<EncodingError> for Error {
    fn from(error: EncodingError) -> Self {
        Self::EncodingError(error)
    }
}

impl From<FlacError> for Error {
    fn from(error: FlacError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::VerifyError> for Error {
    fn from(error: flacenc::error::VerifyError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<(flacenc::config::Encoder, flacenc::error::VerifyError)> for Error {
    fn from(error: (flacenc::config::Encoder, flacenc::error::VerifyError)) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::EncodeError> for Error {
    fn from(error: flacenc::error::EncodeError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::SourceError> for Error {
    fn from(error: flacenc::error::SourceError) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<flacenc::error::OutputError<flacenc::bitsink::ByteSink>> for Error {
    fn from(error: flacenc::error::OutputError<flacenc::bitsink::ByteSink>) -> Self {
        EncodingError::from(error).into()
    }
}

impl From<MetaDataError> for Error {
    fn from(error: MetaDataError) -> Self {
        Self::MetaDataError(error)
    }
}

impl From<UnknownMimeType> for Error {
    fn from(error: UnknownMimeType) -> Self {
        MetaDataError::from(error).into()
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::other(error)
    }
}

impl<R> From<Context<R>> for Error {
    fn from(_: Context<R>) -> Self {
        Self::ThreadCancellation
    }
}
