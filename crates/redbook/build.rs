use build_safely::prelude::*;

include!("./src/bin/rip-cli/cli.rs");

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

    let mut ac = AutoCfg::new()?;

    // check to see any  downstream crate has defined
    // `unstable.allow-features` in `.cargo/config.toml`.
    let allowed_features = cargo_allowed_features()?;

    ac.emit_unstable_feature(const_bool, &allowed_features);
    ac.emit_unstable_feature(const_cmp, &allowed_features);
    ac.emit_unstable_feature(const_convert, &allowed_features);
    ac.emit_unstable_feature(const_destruct, &allowed_features);
    ac.emit_unstable_feature(const_ops, &allowed_features);
    ac.emit_unstable_feature(const_option_ops, &allowed_features);
    ac.emit_unstable_feature(const_result_trait_fn, &allowed_features);
    ac.emit_unstable_feature(const_trait_impl, &allowed_features);
    ac.emit_unstable_feature(const_try, &allowed_features);
    ac.emit_unstable_feature(default_field_values, &allowed_features);
    ac.emit_unstable_feature(error_generic_member_access, &allowed_features);
    ac.emit_unstable_feature(error_iter, &allowed_features);
    ac.emit_unstable_feature(error_reporter, &allowed_features);
    ac.emit_unstable_feature(exact_div, &allowed_features);
    ac.emit_unstable_feature(exact_size_is_empty, &allowed_features);
    ac.emit_unstable_feature(integer_casts, &allowed_features);
    ac.emit_unstable_feature(integer_cast_extras, &allowed_features);
    ac.emit_unstable_feature(iter_array_chunks, &allowed_features);
    ac.emit_unstable_feature(iter_map_windows, &allowed_features);
    ac.emit_unstable_feature(iterator_try_collect, &allowed_features);
    ac.emit_unstable_feature(negative_impls, &allowed_features);
    ac.emit_unstable_feature(os_str_split_at, &allowed_features);
    ac.emit_unstable_feature(path_absolute_method, &allowed_features);
    ac.emit_unstable_feature(ptr_metadata, &allowed_features);
    ac.emit_unstable_feature(raw_os_error_ty, &allowed_features);
    ac.emit_unstable_feature(structural_match, &allowed_features);
    ac.emit_unstable_feature(trait_alias, &allowed_features);
    ac.emit_unstable_feature(try_blocks_heterogeneous, &allowed_features);
    ac.emit_unstable_feature(fn_traits, &allowed_features);
    ac.emit_unstable_feature(offset_of_slice, &allowed_features);

    Ok(())
}
