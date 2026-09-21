use std::io;
use tracing_subscriber::{filter::LevelFilter, fmt::layer, prelude::*, registry};

pub fn init_tracing() -> io::Result<()> {
    let stdout_level = LevelFilter::DEBUG;
    let stdout = layer().with_filter(stdout_level);

    registry()
        .with(stdout)
        .try_init()
        .map_err(io::Error::other)?;

    Ok(())
}
