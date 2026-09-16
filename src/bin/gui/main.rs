mod slint;
use slint::*;

#[cfg(feature = "gui")]
fn main() {
    MainWindow::new().unwrap().run().unwrap()
}

#[cfg(not(feature = "gui"))]
fn main() {}
