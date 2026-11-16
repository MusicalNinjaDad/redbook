//! A logical CD, represented by [`Disc`] with the details you would expect on the printed case.
//!
//! This module uses [musicbrainz](https://musicbrainz.org/) as the source of metadata.
//! 
//! See [Disc], the main entry point, for examples

#[doc(inline)]
pub use disc::{Disc, Tracks};
pub use error::DiscError;
#[doc(inline)]
pub use track::Track;

mod disc;
pub mod error;
mod musicbrainz;
mod track;
