use build_safely::prelude::*;

fn main() -> Result<()> {
    println!("cargo::rerun-if-changed=./src/gui.slint");

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
