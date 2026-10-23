//! Build script that exports the host and target triples, the Cargo target directory and
//! rerun triggers to the `xtask` binary as compile-time environment variables.
use anyhow::Result;
use std::{path::Path, process::Command};
/// Emits the `cargo:` directives for the build script. Fails when the host or target
/// environment variables are missing, or when `cargo metadata` cannot report the target
/// directory.
fn main() -> Result<()> {
    println!("cargo:rustc-env=HOST={}", std::env::var("HOST")?);
    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET")?);
    println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");
    println!("cargo:rerun-if-env-changed=CARGO_BUILD_TARGET_DIR");
    println!("cargo:rerun-if-changed=../.cargo/config.toml");
    let metadata = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["metadata", "--format-version=1", "--no-deps", "--locked"])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join(".."))
        .output()?;
    anyhow::ensure!(
        metadata.status.success(),
        "Could not resolve Cargo's target directory: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&metadata.stdout)?;
    let target_dir = metadata["target_directory"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Cargo metadata has no target directory"))?;
    println!("cargo:rustc-env=TARGET_DIRECTORY={target_dir}");
    Ok(())
}
