//! Renders the parity scenes with a head and a base build of the CLI and writes the images,
//! timings and Markdown comment that the pull request preview workflow publishes.
mod compare;
mod markdown;
mod render;
mod sweep;
mod workload;

use crate::util::crate_root;
use anyhow::{Context, Result, ensure};
use std::{fs, path::PathBuf};

/// Frames rendered per workload: the first `render::WARMUP` are discarded and the rest are
/// timed. This is shorter than the `bench` protocol because CI runs on a software adapter.
const DEFAULT_FRAMES: usize = 7;

/// Arguments of `cargo xtask preview`.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// CLI binary built from the pull request.
    #[arg(long)]
    head: PathBuf,
    /// CLI binary built from the base branch; without it nothing is compared.
    #[arg(long)]
    base: Option<PathBuf>,
    /// Folder that receives the images and `comment.md`.
    #[arg(long)]
    out: PathBuf,
    /// URL prefix under which the contents of `--out` will be reachable from the comment.
    #[arg(long)]
    image_base_url: String,
    /// Address of the web viewer built from the pull request, linked from the comment.
    #[arg(long)]
    web_url: Option<String>,
    /// Frames rendered per workload, including the discarded warm-up frames.
    #[arg(long, default_value_t = DEFAULT_FRAMES)]
    frames: usize,
}

/// Renders every workload with the head build, and with the base build when one is given,
/// compares the results and writes `comment.md` into the output folder.
///
/// # Errors
/// Fails when a binary is missing, a render fails, a report is malformed or the two builds
/// produce images of different sizes.
pub fn run(args: &Args) -> Result<()> {
    ensure!(
        args.frames > render::WARMUP,
        "--frames must exceed the {} warm-up frames",
        render::WARMUP
    );
    let root = crate_root();
    let workloads = workload::load(&root)?;
    fs::create_dir_all(&args.out).with_context(|| format!("Creating {}", args.out.display()))?;
    let mut rows = Vec::new();
    for workload in &workloads {
        println!("Rendering {}", workload.name);
        let head = render::render(&args.head, workload, &args.out.join("head"), args.frames)?;
        let base = args
            .base
            .as_ref()
            .map(|base| render::render(base, workload, &args.out.join("base"), args.frames))
            .transpose()?;
        let difference = base
            .as_ref()
            .map(|base| {
                compare::compare(
                    &base.exr,
                    &head.exr,
                    &args.out.join("diff").join(format!("{}.png", workload.name)),
                )
            })
            .transpose()?;
        rows.push(markdown::Row {
            name: workload.name.clone(),
            head,
            base,
            difference,
        });
    }
    let sweep = sweep::run(&root, &args.head, args.base.as_deref(), &args.out)?;
    fs::write(
        args.out.join("comment.md"),
        markdown::comment(
            &rows,
            Some(&sweep),
            &args.image_base_url,
            args.web_url.as_deref(),
        ),
    )?;
    println!("Preview written to {}", args.out.display());
    Ok(())
}
