//! Parity comparison of the OFX plugin: the baselines are rendered by the CLI of this checkout, so nothing is stored outside the repository.
use crate::crate_root;
use anyhow::Result;
use duct::cmd;

/// Renders every scene of `test/parity/commands.tsv` with the release CLI into `target/parity/exr`, then renders the same scenes through the plugin and compares them. Both run on this machine, because baselines differ between GPUs.
pub fn run_parity() -> Result<()> {
    let root = crate_root();
    cmd(
        "cargo",
        [
            "build",
            "--release",
            "--locked",
            "--package",
            "opendefocus-cli",
        ],
    )
    .dir(&root)
    .run()?;
    cmd(
        "cargo",
        [
            "run",
            "--release",
            "--locked",
            "--package",
            "opendefocus-cli",
            "--example",
            "compare_renders",
            "--",
            "--parity",
            "test/parity",
            "--cli",
            "target/release/opendefocus-cli",
            "--out",
            "target/parity/exr",
            "--render",
        ],
    )
    .dir(&root)
    .run()?;
    cmd(
        "cargo",
        [
            "test",
            "--release",
            "--locked",
            "--package",
            "opendefocus-ofx",
            "--",
            "--ignored",
            "--test-threads=1",
            "parity",
        ],
    )
    .dir(&root)
    .run()?;
    Ok(())
}
