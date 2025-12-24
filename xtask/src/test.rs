use anyhow::Result;
use duct::cmd;

use crate::util::crate_root;
pub async fn test_crates() -> Result<()> {
    cmd!("cargo", "test", "--release").run()?; // we really need the 'release' tag else it will take a long time because of optimizations in ndarray
    Ok(())
}

pub async fn test_nuke_plugin_package() -> Result<()> {
    cmd!(
        "uv",
        "run",
        "--isolated",
        "--no-project",
        "--with",
        "pytest",
        "pytest",
        crate_root().join("test").join("test_package")
    )
    .env("PYTHONPATH", crate_root().join("package"))
    .run()?;

    Ok(())
}
