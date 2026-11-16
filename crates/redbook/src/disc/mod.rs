//! Disc metadata and MusicBrainz integration
//!
//! Provides the [`Disc`] struct for representing a physical CD, including its table of contents,
//! tracks, and metadata retrieved from MusicBrainz and CoverArtArchive.

#[doc(inline)]
pub use disc::{Disc, Tracks};
pub use error::DiscError;
#[doc(inline)]
pub use track::Track;

mod disc;
pub mod error;
mod musicbrainz;
mod track;
