use build_safely::prelude::*;

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed=./src/gui.slint");

    // Embed the application icon into the `rip` binary on Windows.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = std::path::Path::new("assets/icon.ico");
        if icon.exists() {
            winresource::WindowsResource::new()
                .set_icon(icon.to_str().unwrap())
                .compile()?;
        } else {
            println!("cargo::warning=assets/icon.ico not found - building without an embedded icon (generate it from assets/icon.svg, see assets/README.md)");
        }
    }

    slint_build::compile("./src/gui.slint").map_err(|err| BuildError::Other(err.to_string()))?;

    let mut ac = AutoCfg::new()?;

    // check to see any  downstream crate has defined
    // `unstable.allow-features` in `.cargo/config.toml`.
    let allowed_features = cargo_allowed_features()?;

    ac.emit_unstable_feature(integer_casts, &allowed_features);
    ac.emit_unstable_feature(try_blocks, &allowed_features);
    ac.emit_unstable_feature(try_blocks_heterogeneous, &allowed_features);

    Ok(())
}
