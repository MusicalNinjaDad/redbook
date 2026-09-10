#![cfg_attr(
    not(target_family = "windows"),
    expect(unused_imports, reason = "stubs")
)]

//! List available CD drives

use std::io;

#[cfg(target_family = "windows")]
use redbook::win::drive::{CdDrive, all_drives};
use tracing::level_filters::LevelFilter;
#[cfg(target_family = "windows")]
use tracing_result::Trace;
use tracing_subscriber::{
    Layer, fmt::layer, layer::SubscriberExt, registry, util::SubscriberInitExt,
};

#[cfg(target_family = "windows")]
fn main() -> io::Result<()> {
    registry()
        .with(layer().with_filter(LevelFilter::DEBUG))
        .init();

    let drives: Vec<CdDrive> = all_drives().collect();

    for cd in drives {
        let path = cd.path();
        let toc = cd.toc().as_toc().map_err(io::Error::other).or_warn("")?;
        tracing::info!(path = %path.display(), %toc, "found");
    }
    Ok(())
}

#[cfg(not(target_family = "windows"))]
fn main() {}
