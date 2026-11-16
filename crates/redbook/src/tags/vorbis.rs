use std::path::PathBuf;

use metaflac::block::{Picture, PictureType};
use tracing_result::Trace;

use crate::{disc::tagging::ImageMimeTypes, error::UnknownMimeType};

use super::sanitise;

/// File naming logic for the cover art [`Picture`] of a [`RippedTrack`].
pub(crate) trait PictureExt {
    /// The filename for the picture, based on its picture type & mime type (e.g. `cover.jpg`)
    fn filename(&self) -> Result<PathBuf, UnknownMimeType>;
}

impl PictureExt for Picture {
    fn filename(&self) -> Result<PathBuf, UnknownMimeType> {
        let _debug_span = tracing::debug_span!("PictureExt::filename").entered();
        let filename = match self.picture_type {
            PictureType::CoverFront => "cover",
            _ => todo!("TODO #100: handle other picture types"),
        };
        let extension = match self
            .mime_type
            .parse::<ImageMimeTypes>()
            .or_warn("identifying image mime type")?
        {
            ImageMimeTypes::Jpeg => "jpg",
        };
        let path = PathBuf::from(sanitise(filename)).with_extension(sanitise(extension));
        tracing::debug!(path = %path.display());
        Ok(path)
    }
}
