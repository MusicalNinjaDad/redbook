#![cfg_attr(unstable_try_blocks_heterogeneous, feature(try_blocks_heterogeneous))]

#[cfg(feature = "gui")]
mod album;

mod slint;
use slint::*;

#[cfg(feature = "gui")]
fn main() {
    MainWindow::new().unwrap().run().unwrap()
}

#[cfg(not(feature = "gui"))]
fn main() {}
