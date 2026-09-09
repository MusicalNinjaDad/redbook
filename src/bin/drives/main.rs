//! List available CD drives

use std::io;

use redbook::win::drive::{_get_drive_infosets, _list_drives};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{
    Layer, fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt,
};

#[cfg(target_family = "windows")]
fn main() -> io::Result<()> {
    registry()
        .with(layer().with_filter(LevelFilter::DEBUG))
        .init();
    let devs = _get_drive_infosets()?;
    _list_drives(devs)?;
    Ok(())
}

#[cfg(not(target_family = "windows"))]
fn main() {}
