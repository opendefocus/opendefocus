//! Runs the workspace test suite for `cargo xtask --test-crates`.
use anyhow::Result;
use duct::cmd;

use crate::util::crate_root;
/// Runs `cargo test --release --workspace --locked` from the repository root. Release mode
/// is used because image-processing tests are too slow in debug builds. Fails when any
/// test fails.
pub async fn test_crates() -> Result<()> {
    cmd!("cargo", "test", "--release", "--workspace", "--locked")
        .dir(crate_root())
        .run()?;
    Ok(())
}
