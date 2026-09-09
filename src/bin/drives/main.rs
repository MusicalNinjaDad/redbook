//! List available CD drives

use std::io;

use redbook::win::drive::_get_drive_infosets;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{Layer, fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt};

fn main() -> io::Result<()> {
    registry().with(layer().with_filter(LevelFilter::DEBUG)).init();
    _get_drive_infosets()?;
    Ok(())
}
