use std::{fmt::Display, str::FromStr};

use metaflac::block::{Picture, PictureType};
use tracing_result::Trace;
use zune_jpeg::{JpegDecoder, zune_core::bytestream::ZCursor};

use crate::error::UnknownMimeType;

/// The image mime types supported for cover art.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ImageMimeTypes {
    /// jpeg image data
    Jpeg,
}

impl Display for ImageMimeTypes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "image/")?;
        match self {
            ImageMimeTypes::Jpeg => write!(f, "jpeg"),
        }
    }
}

impl FromStr for ImageMimeTypes {
    type Err = UnknownMimeType;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mimetype = match s {
            "image/jpeg" => ImageMimeTypes::Jpeg,
            _ => Err(UnknownMimeType::from(s)).or_warn("parsing artwork mime type")?,
        };
        Ok(mimetype)
    }
}

/// Construct a [`Picture`] block of `picture_type` from jpeg `data`, reading the image
/// dimensions from the jpeg headers.
pub(crate) fn picture_from_jpeg<B: AsRef<[u8]>, S: ToString>(
    picture_type: PictureType,
    description: S,
    data: B,
) -> Picture {
    let mut jpg_info = JpegDecoder::new(ZCursor::from(&data));

    #[expect(unused_must_use, reason = "TODO: change to try_from_jpeg")]
    jpg_info.decode_headers();

    let width = jpg_info.info().unwrap().width as u32;
    let height = jpg_info.info().unwrap().height as u32;

    Picture {
        picture_type,
        mime_type: ImageMimeTypes::Jpeg.to_string(),
        description: description.to_string(),
        width,
        height,
        depth: 24,
        num_colors: 0,
        data: data.as_ref().to_vec(),
    }
}
