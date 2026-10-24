//! Release tasks: builds the OpenFX bundle and generates the documentation, including the
//! downloads page.
use crate::{
    license::fetch_licenses,
    util::{crate_root, target_directory},
};
use anyhow::{Context, Result, bail};
use duct::cmd;
use std::path::PathBuf;

mod downloads;

/// Builds the `OpenFX` plugin for the host platform as `OpenDefocus.ofx.bundle` in the target
/// directory. The bundle layout follows the `OpenFX` packaging rules: the binary sits in
/// `Contents/<arch>/OpenDefocus.ofx`, and macOS bundles also carry `Contents/Info.plist`.
pub fn bundle_ofx() -> Result<PathBuf> {
    cmd(
        "cargo",
        [
            "build",
            "--release",
            "--locked",
            "--package",
            "opendefocus-ofx",
        ],
    )
    .dir(crate_root())
    .run()?;
    let built = target_directory().join("release").join(format!(
        "{}opendefocus_ofx{}",
        std::env::consts::DLL_PREFIX,
        std::env::consts::DLL_SUFFIX
    ));
    let arch = ofx_arch_folder()?;
    let bundle = target_directory()
        .join("ofx")
        .join("OpenDefocus.ofx.bundle");
    if bundle.exists() {
        std::fs::remove_dir_all(&bundle)
            .with_context(|| format!("Removing old bundle {}", bundle.display()))?;
    }
    let contents = bundle.join("Contents");
    let binary_dir = contents.join(arch);
    std::fs::create_dir_all(&binary_dir)?;
    std::fs::copy(&built, binary_dir.join("OpenDefocus.ofx"))
        .with_context(|| format!("Copying {}", built.display()))?;
    // Hosts look for `<plugin identifier>.png` in Resources to show the plugin icon.
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&resources)?;
    std::fs::copy(
        crate_root()
            .join("packaging")
            .join("ofx")
            .join("org.opendefocus.OpenDefocus.png"),
        resources.join("org.opendefocus.OpenDefocus.png"),
    )
    .context("Copying the plugin icon")?;
    if std::env::consts::OS == "macos" {
        std::fs::write(contents.join("Info.plist"), info_plist())?;
    }
    log::info!("OpenFX bundle written to {}", bundle.display());
    Ok(bundle)
}

/// Architecture folder inside `Contents` for the host, per <https://openfx.readthedocs.io>
/// (Packaging): `MacOS` for macOS universal binaries, `Linux-${uname -m}` on Linux, `Win64`
/// for 64-bit Windows.
fn ofx_arch_folder() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => Ok("MacOS"),
        ("linux", "x86_64") => Ok("Linux-x86-64"),
        ("linux", "aarch64") => Ok("Linux-aarch64"),
        ("windows", "x86_64") => Ok("Win64"),
        (os, arch) => bail!("No OpenFX bundle layout for {os}-{arch}"),
    }
}

/// `Info.plist` for the macOS bundle. The executable is the renamed cdylib.
fn info_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>English</string>
    <key>CFBundleExecutable</key>
    <string>OpenDefocus.ofx</string>
    <key>CFBundleIdentifier</key>
    <string>org.opendefocus.OpenDefocus</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>OpenDefocus</string>
    <key>CFBundlePackageType</key>
    <string>BNDL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
</dict>
</plist>
"#,
        version = env!("CARGO_PKG_VERSION")
    )
}

/// Builds the documentation: prepares the generated pages, then runs `mdbook build` on the
/// `docs` folder.
pub async fn release_docs() -> Result<()> {
    prepare_docs().await?;
    cmd!("mdbook", "build", crate_root().join("docs")).run()?;
    Ok(())
}

/// Generates the documentation pages that are not kept in Git: the license notice and the
/// downloads table.
pub async fn prepare_docs() -> Result<()> {
    fetch_licenses(crate_root().join("docs").join("src").join("licenses.md")).await?;
    downloads::create_downloads_table().await?;
    Ok(())
}

/// Tests for the OpenFX bundle layout.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the OpenFX architecture folder and the macOS `Info.plist` keys.
    #[test]
    fn ofx_bundle_layout_matches_openfx_folders() {
        assert!(ofx_arch_folder().is_ok());
        let plist = info_plist();
        assert!(plist.contains("<string>OpenDefocus.ofx</string>"));
        assert!(plist.contains("<string>BNDL</string>"));
    }
}
