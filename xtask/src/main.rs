//! Entry point of `cargo xtask`. It parses the command line, then runs each requested
//! build, test, packaging, release or benchmark task in a fixed order.
mod bench;
mod compile;
mod consts;
mod fixtures;
mod license;
mod natron;
mod package;
mod parity;
mod precommit;
mod release;
mod test;
mod util;

use std::path::PathBuf;

use crate::{
    compile::{compile_cli, compile_ui, compile_web, run_ui, validate_gpu_shaders},
    license::fetch_licenses,
    natron::test_natron,
    package::package_ofx,
    precommit::{clippy_all, precommit},
    release::{bundle_ofx, prepare_docs, release_docs, release_package},
    test::test_crates,
    util::crate_root,
};
use anyhow::Result;
use clap::{ArgAction, ArgGroup, Parser};
use duct::cmd;

/// Command-line interface of `cargo xtask`. Each flag selects one task; several flags
/// can be combined in one invocation.
#[derive(Parser, Debug)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "each bool is an independent command-line flag"
)]
#[command(version, about, long_about = None, arg_required_else_help = true)]
#[command(group(ArgGroup::new("build_kind").args(["ui", "web", "cli", "gpu"]).multiple(true)))]
struct Args {
    /// Optional subcommand (`bench` or `fixtures`). When present it runs alone and the
    /// top-level task flags are not executed.
    #[command(subcommand)]
    command: Option<Command>,

    /// Compile the components selected with `--ui`, `--web`, `--cli` or `--gpu`.
    #[clap(short, long, action=ArgAction::SetTrue, requires = "build_kind")]
    compile: bool,

    /// Build the standalone evaluation UI for the host platform
    #[arg(long, requires = "compile")]
    ui: bool,

    /// Build the WebAssembly viewer and static files in target/web
    #[arg(long, requires = "compile", conflicts_with_all = ["ui", "cli"])]
    web: bool,

    /// Build the standalone evaluation CLI for the host platform
    #[arg(long, requires = "compile")]
    cli: bool,

    /// Validate the native WGSL shaders
    #[clap(short, long, action=ArgAction::SetTrue, requires = "compile")]
    gpu: bool,

    /// Build the `OpenFX` bundle `OpenDefocus.ofx.bundle` for the host platform
    #[clap(long, action=ArgAction::SetTrue)]
    bundle_ofx: bool,

    /// Build the `OpenFX` bundle and wrap it in an installer for the host platform
    #[clap(long, action=ArgAction::SetTrue)]
    package_ofx: bool,

    /// Ship the package folder to a public release
    #[clap(long, action=ArgAction::SetTrue)]
    release_package: bool,

    /// Destination file for the archive written by `--release-package`. When omitted the
    /// archive goes to a versioned zip in the system temporary directory.
    #[clap(long, requires = "release_package")]
    target_package_path: Option<PathBuf>,

    /// Build documentation for GitHub Pages
    #[clap(long, action=ArgAction::SetTrue)]
    release_docs: bool,

    /// Write the dependency license notice to this path (see `fetch_licenses`).
    #[clap(long)]
    create_licenses: Option<PathBuf>,

    /// Build the documentation and serve it locally with `mdbook serve`.
    #[clap(long, action=ArgAction::SetTrue)]
    serve_docs: bool,

    /// Run the workspace test suite in release mode.
    #[clap(long, action=ArgAction::SetTrue)]
    test_crates: bool,

    /// Build the OFX bundle and compare its occlusion render in `NatronRenderer` with the CLI
    /// (needs Natron; set `NATRON_RENDERER` or install `Natron.app`). Hosted runners may lack a GPU.
    #[clap(long, action=ArgAction::SetTrue)]
    test_natron: bool,

    /// Run the pre-commit checks. The trailing arguments are passed to them.
    #[clap(long, action=ArgAction::SetTrue)]
    precommit: bool,

    /// Run clippy on the workspace and on every package alone, denying warnings
    #[clap(long, action=ArgAction::SetTrue)]
    clippy: bool,

    /// Introduces the `--args` flag. Its value is not read; trailing arguments are collected
    /// by `remaining`.
    #[arg(long = "args")]
    passthrough: bool,

    /// Arguments that follow the flags, forwarded to the `precommit` task.
    remaining: Vec<String>,
}

/// Subcommands of `cargo xtask`.
#[derive(clap::Subcommand, Debug)]
enum Command {
    /// Compare release render timings before and after development
    Bench(bench::Args),
    /// Generate and verify test fixtures and run the quality gates
    #[command(subcommand)]
    Fixtures(fixtures::Fixtures),
    /// Render the parity scenes with the CLI and compare the OFX plugin against them on this machine
    Parity,
    /// Build the standalone evaluation viewer in release mode and launch it
    Ui {
        /// Arguments passed to the viewer
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        arguments: Vec<String>,
    },
}

/// Validates a Nuke version string such as `16.0`: two non-empty numeric parts separated
/// by one dot. Returns the input unchanged when it is valid; otherwise returns the
/// message shown to the user.
fn parse_nuke_version(value: &str) -> std::result::Result<String, String> {
    let valid = value.split_once('.').is_some_and(|(major, minor)| {
        [major, minor].iter().all(|part| {
            !part.is_empty()
                && part.bytes().all(|c| c.is_ascii_digit())
                && part.parse::<u32>().is_ok()
        })
    });
    if !valid {
        return Err("Nuke versions must use major.minor, for example 16.0".into());
    }
    Ok(value.to_owned())
}

/// Parses the command line and runs the requested tasks in order. The first task that
/// fails returns its error, so later tasks do not run.
#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .compact()
        .with_line_number(false)
        .with_file(false)
        .init();
    let args = Args::parse();

    match args.command {
        Some(Command::Bench(bench)) => return bench::run(bench),
        Some(Command::Fixtures(fixtures)) => return fixtures::run(fixtures),
        Some(Command::Parity) => return parity::run_parity(),
        Some(Command::Ui { arguments }) => return run_ui(&arguments),
        None => {}
    }

    if args.test_crates {
        test_crates().await?;
    }

    if args.test_natron {
        test_natron()?;
    }

    if args.clippy {
        clippy_all()?;
    }

    if args.precommit {
        precommit(args.remaining).await?;
    }

    if args.compile {
        if args.gpu {
            validate_gpu_shaders().await?;
        }
        if args.cli {
            compile_cli()?;
        }
        if args.web {
            compile_web()?;
        }
        if args.ui {
            compile_ui()?;
        }
    }

    if let Some(licenses_path) = args.create_licenses {
        fetch_licenses(licenses_path).await?;
    }

    if args.serve_docs {
        prepare_docs().await?;
        cmd!("mdbook", "serve", crate_root().join("docs")).run()?;
    }

    if args.bundle_ofx {
        bundle_ofx()?;
    }

    if args.package_ofx {
        package_ofx()?;
    }

    if args.release_package {
        release_package(args.target_package_path).await?;
    }

    if args.release_docs {
        release_docs().await?;
    }

    Ok(())
}

/// Parser tests for the command-line interface.
#[cfg(test)]
mod argument_tests {
    use super::*;

    /// Verifies that incomplete task combinations, such as `--compile` alone, are rejected.
    #[test]
    fn test_incomplete_tasks_are_rejected() {
        for args in [
            vec!["xtask", "--compile"],
            vec!["xtask", "--gpu"],
            vec!["xtask", "--target-package-path", "release.zip"],
        ] {
            assert!(
                Args::try_parse_from(&args).is_err(),
                "Accepted incomplete task: {args:?}"
            );
        }
    }

    /// Verifies that the `bench` subcommand parses and that `--compile` with `--cli` or
    /// `--web` is accepted while `--web` without `--compile` is rejected.
    #[test]
    fn test_benchmark_subcommand_preserves_existing_build_flags() {
        assert!(matches!(
            Args::try_parse_from(["xtask", "bench", "after"])
                .unwrap()
                .command,
            Some(Command::Bench(_))
        ));
        let args = Args::try_parse_from(["xtask", "--compile", "--cli"]).unwrap();
        assert!(args.compile && args.cli && args.command.is_none());
        assert!(
            Args::try_parse_from(["xtask", "--compile", "--web"])
                .unwrap()
                .web
        );
        assert!(Args::try_parse_from(["xtask", "--web"]).is_err());
        assert!(Args::try_parse_from(["xtask", "--compile", "--web", "--ui"]).is_err());
    }
}
