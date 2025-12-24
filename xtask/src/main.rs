mod compile;
mod consts;
mod license;
mod nuke;
mod precommit;
mod release;
mod test;
mod util;

use core::fmt;
use std::path::PathBuf;

use crate::{
    compile::compile_spirv,
    license::fetch_licenses,
    nuke::{compile_nuke, create_package, get_sources},
    precommit::precommit,
    release::{release_docs, release_package},
    test::{test_crates, test_nuke_plugin_package},
    util::crate_root,
};
use anyhow::Result;
use clap::{ArgAction, Parser};
use duct::cmd;

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq)]
pub enum TargetPlatform {
    Windows,
    Linux,
    MacosX86_64,
    MacosAarch64,
}

impl fmt::Display for TargetPlatform {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TargetPlatform::Windows => write!(f, "x86_64-windows"),
            TargetPlatform::Linux => write!(f, "x86_64-linux"),
            TargetPlatform::MacosX86_64 => write!(f, "x86_64-macos"),
            TargetPlatform::MacosAarch64 => write!(f, "aarch64-macos"),
        }
    }
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
struct Args {
    #[clap(short, long, action=ArgAction::SetTrue)]
    compile: bool,

    /// Compile using zig
    #[clap(long, action=ArgAction::SetTrue)]
    use_zig: bool,

    /// If compilation of spirv is needed
    #[clap(short, long, action=ArgAction::SetTrue)]
    gpu: bool,

    #[clap(short, long)]
    target_platform: Option<TargetPlatform>,

    /// Ship the package folder to a public release
    #[clap(long, action=ArgAction::SetTrue)]
    release_package: bool,

    #[clap(long)]
    target_package_path: Option<PathBuf>,

    /// Release the docs onto the repository
    #[clap(long, action=ArgAction::SetTrue)]
    release_docs: bool,

    #[clap(short, long, value_delimiter = ',')]
    nuke_versions: Vec<String>,

    #[clap(short, long, action=ArgAction::SetTrue)]
    fetch_nuke: bool,

    #[clap(long, action=ArgAction::SetTrue)]
    output_to_package: bool,

    #[clap(long)]
    create_licenses: Option<PathBuf>,

    #[clap(long, action=ArgAction::SetTrue)]
    serve_docs: bool,

    #[clap(long, action=ArgAction::SetTrue)]
    test_crates: bool,

    #[clap(long, action=ArgAction::SetTrue)]
    pytest: bool,

    #[clap(long, action=ArgAction::SetTrue)]
    test_nuke_plugin_package: bool,

    #[clap(long, action=ArgAction::SetTrue)]
    precommit: bool,

    #[arg(long)]
    args: bool,

    remaining: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_line_number(false)
        .with_file(false)
        .init();
    let args = Args::parse();

    if args.test_crates {
        test_crates().await?;
    }

    if args.pytest {
        if args.test_nuke_plugin_package {
            test_nuke_plugin_package().await?;
        }
    }

    if args.precommit {
        precommit(args.remaining).await?;
    }

    if args.fetch_nuke
        && let Some(target_platform) = args.target_platform
    {
        get_sources(
            vec![target_platform],
            args.nuke_versions.clone(),
        )
        .await?;
    }
    if args.compile {
        if args.gpu {
            compile_spirv().await?;
        }
        if !args.nuke_versions.is_empty()
            && let Some(target_platform) = args.target_platform
        {
            compile_nuke(
                args.nuke_versions.clone(),
                target_platform,
                args.use_zig,
            )
            .await?;
        }
    }
    if args.output_to_package
        && let Some(target_platform) = args.target_platform
    {
        create_package(target_platform, args.nuke_versions.clone()).await?;
    }

    if let Some(licenses_path) = args.create_licenses {
        fetch_licenses(licenses_path).await?;
    }

    if args.serve_docs {
        cmd!("mdbook", "serve", crate_root().join("docs")).run()?;
    }

    if args.release_package {
        release_package(args.target_package_path).await?;
    }

    if args.release_docs {
        release_docs().await?;
    }

    Ok(())
}
