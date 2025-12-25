//! Simple executable to be able to compile the spirv bytecode with a specific nightly toolchain
#![warn(unused_extern_crates)]
use std::path::PathBuf;

use anyhow::Result;
use clap::{ArgAction, Parser};
use spirv_builder::SpirvBuilder;

const TOOLCHAIN_VERSION: &str = "nightly-2025-06-30";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
struct Args {
    /// Path of crate to build
    #[clap(short, long)]
    crate_path: PathBuf,
    /// Path where to write spirv file to
    #[clap(short, long)]
    output: PathBuf,

    /// Return the path to the compiled file
    #[clap(short, long, action=ArgAction::SetTrue)]
    print_output: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let mut build = SpirvBuilder::new(args.crate_path, "spirv-unknown-vulkan1.4")
        .target_dir_path(args.output)
        .print_metadata(spirv_builder::MetadataPrintout::Full)
        .spirv_metadata(spirv_builder::SpirvMetadata::Full)
        .shader_crate_default_features(false)
        .capability(spirv_builder::Capability::Int8)
        .shader_crate_features(["libm".to_string()]);

    build.toolchain_overwrite = Some(TOOLCHAIN_VERSION.to_string());
    let compilation = build.build()?;
    if args.print_output {
        println!("{}", compilation.module.unwrap_single().display());
    }

    Ok(())
}
