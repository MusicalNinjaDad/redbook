use build_safely::prelude::*;

include!("./src/bin/rip/cli.rs");

fn main() -> Result<()> {
    if get_var("PROFILE")? == "release" {
        use clap_builder::{CommandFactory, ValueEnum};
        use clap_complete::Shell;

        let mut cmd = Rip::command();

        let out_dir = std::path::PathBuf::from(get_var("OUT_DIR")?);
        let bin_name = get_var("CARGO_PKG_NAME")?;

        clap_mangen::generate_to(cmd.clone(), &out_dir)?;

        for &shell in Shell::value_variants() {
            clap_complete::generate_to(shell, &mut cmd, &bin_name, &out_dir)?;
        }
    }

    let ac = AutoCfg::new()?;

    // check to see any  downstream crate has defined
    // `unstable.allow-features` in `.cargo/config.toml`.
    let allowed_features = cargo_allowed_features()?;

    ac.emit_unstable_feature(OtherFeature("exact_div".to_string()), &allowed_features);
    ac.emit_unstable_feature(
        OtherFeature("exact_size_is_empty".to_string()),
        &allowed_features,
    );
    ac.emit_unstable_feature(
        OtherFeature("iter_array_chunks".to_string()),
        &allowed_features,
    );
    ac.emit_unstable_feature(iterator_try_collect, &allowed_features);
    ac.emit_unstable_feature(
        OtherFeature("negative_impls".to_string()),
        &allowed_features,
    );
    ac.emit_unstable_feature(
        OtherFeature("path_absolute_method".to_string()),
        &allowed_features,
    );
    ac.emit_unstable_feature(OtherFeature("try_blocks".to_string()), &allowed_features);

    Ok(())
}
