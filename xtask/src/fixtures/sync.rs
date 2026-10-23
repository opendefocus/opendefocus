//! Stages and verifies the packaged copies of the bokeh and circle-of-confusion fixtures
//! for `cargo xtask fixtures sync`.
mod bokeh;
mod bokeh_package;
mod circle_of_confusion;

use anyhow::Result;

use crate::util::{crate_root, sha256_file};

/// Options of `cargo xtask fixtures sync`. Without `--stage` the packaged copies are only
/// verified.
#[derive(clap::Args, Debug, Clone, Copy)]
pub struct Args {
    /// Verify the packaged copies against the canonical references without writing (default)
    #[arg(long, conflicts_with_all = ["stage", "test_bokeh_package"])]
    check: bool,

    /// Copy the verified canonical references into the packaged crates
    #[arg(long, conflicts_with = "test_bokeh_package")]
    stage: bool,

    /// Package `bokeh-creator` and run its tests from the extracted crate outside the workspace
    #[arg(long)]
    test_bokeh_package: bool,
}

/// Verifies the bokeh and circle-of-confusion fixtures. With `stage` set, the canonical
/// files are copied into the packages first. With `test_bokeh_package` set, the packaged
/// bokeh crate is tested instead. Fails on any mismatch.
pub fn run(args: Args) -> Result<()> {
    let root = crate_root();
    if args.test_bokeh_package {
        return bokeh_package::test(&root);
    }
    bokeh::sync(&root, args.stage)?;
    circle_of_confusion::sync(&root, args.stage)
}
