//! Installers for the `OpenFX` bundle built by `bundle_ofx`.
//!
//! macOS produces an unsigned `.pkg`, Linux a `.tar.gz` with `install.sh`/`uninstall.sh`,
//! and Windows an Inno Setup `.exe` compiled with `ISCC.exe`. Signing happens in the
//! release workflow, not here.

use crate::{
    release::bundle_ofx,
    util::{crate_root, target_directory},
};
use anyhow::{Context, Result, bail, ensure};
use duct::cmd;
use std::path::{Path, PathBuf};

/// File name of the OpenFX bundle directory inside the installer payload.
const BUNDLE_NAME: &str = "OpenDefocus.ofx.bundle";
/// Package identifier for the macOS component and product archive.
const MACOS_IDENTIFIER: &str = "org.opendefocus.OpenDefocus.ofx";
/// Directory the macOS `OpenFX` host scans; the bundle is installed inside it.
const MACOS_INSTALL_LOCATION: &str = "/Library/OFX/Plugins";

/// Builds the `OpenFX` bundle, then wraps it in the installer for the host platform.
/// Returns the installer path under `target/ofx-installers/`.
pub fn package_ofx() -> Result<PathBuf> {
    let bundle = bundle_ofx()?;
    let os = std::env::consts::OS;
    let name = installer_name(os, std::env::consts::ARCH, env!("CARGO_PKG_VERSION"))?;
    let out_dir = target_directory().join("ofx-installers");
    let staging = target_directory().join("ofx-staging");
    std::fs::create_dir_all(&out_dir)?;
    if staging.exists() {
        std::fs::remove_dir_all(&staging)
            .with_context(|| format!("Removing old staging {}", staging.display()))?;
    }
    std::fs::create_dir_all(&staging)?;
    let output = out_dir.join(&name);
    if output.exists() {
        std::fs::remove_file(&output)?;
    }
    match os {
        "macos" => package_macos(&bundle, &staging, &output)?,
        "linux" => package_linux(&bundle, &staging, &output)?,
        "windows" => package_windows(&bundle, &output)?,
        other => bail!("No OFX installer for {other}"),
    }
    std::fs::remove_dir_all(&staging).ok();
    ensure!(
        output.is_file(),
        "Installer was not written to {}",
        output.display()
    );
    log::info!("OFX installer written to {}", output.display());
    Ok(output)
}

/// File name of the installer for an OS/arch pair, e.g. `OpenDefocus-OFX-0.1.10-macos-arm64.pkg`.
/// macOS reports `aarch64` as `arm64`; Linux and Windows keep the Rust arch name.
pub fn installer_name(os: &str, arch: &str, version: &str) -> Result<String> {
    Ok(match os {
        "macos" => {
            let arch = if arch == "aarch64" { "arm64" } else { arch };
            format!("OpenDefocus-OFX-{version}-macos-{arch}.pkg")
        }
        "linux" => format!("OpenDefocus-OFX-{version}-linux-{arch}.tar.gz"),
        "windows" => format!("OpenDefocus-OFX-{version}-windows-{arch}-setup.exe"),
        _ => bail!("No OFX installer for {os}-{arch}"),
    })
}

/// Builds a component package, then a product archive, that installs the bundle into
/// `/Library/OFX/Plugins`. The package is unsigned.
fn package_macos(bundle: &Path, staging: &Path, output: &Path) -> Result<()> {
    let version = env!("CARGO_PKG_VERSION");
    let payload = staging.join("payload");
    copy_dir(bundle, &payload.join(BUNDLE_NAME))?;
    let component_plist = staging.join("component.plist");
    cmd!(
        "pkgbuild",
        "--analyze",
        "--root",
        payload.as_os_str(),
        component_plist.as_os_str()
    )
    .run()?;
    // Without this, pkgbuild marks the bundle relocatable and the installer may place it
    // wherever an existing copy is found instead of /Library/OFX/Plugins.
    cmd!(
        "plutil",
        "-replace",
        "0.BundleIsRelocatable",
        "-bool",
        "NO",
        component_plist.as_os_str()
    )
    .run()?;
    let component = staging.join("component.pkg");
    cmd!(
        "pkgbuild",
        "--root",
        payload.as_os_str(),
        "--component-plist",
        component_plist.as_os_str(),
        "--identifier",
        MACOS_IDENTIFIER,
        "--version",
        version,
        "--install-location",
        MACOS_INSTALL_LOCATION,
        "--ownership",
        "recommended",
        component.as_os_str()
    )
    .run()?;
    cmd!(
        "productbuild",
        "--package",
        component.as_os_str(),
        output.as_os_str()
    )
    .run()?;
    Ok(())
}

/// Packs the bundle with the POSIX `install.sh` and `uninstall.sh` into a gzip tarball.
fn package_linux(bundle: &Path, staging: &Path, output: &Path) -> Result<()> {
    let scripts = crate_root().join("packaging").join("linux");
    copy_dir(bundle, &staging.join(BUNDLE_NAME))?;
    for script in ["install.sh", "uninstall.sh"] {
        std::fs::copy(scripts.join(script), staging.join(script))
            .with_context(|| format!("Copying {script}"))?;
    }
    cmd!(
        "tar",
        "-czf",
        output.as_os_str(),
        "-C",
        staging.as_os_str(),
        BUNDLE_NAME,
        "install.sh",
        "uninstall.sh"
    )
    .run()?;
    Ok(())
}

/// Compiles the Inno Setup script with `ISCC.exe`. Errors clearly when it is not installed.
fn package_windows(bundle: &Path, output: &Path) -> Result<()> {
    let iscc = find_iscc().context(
        "Inno Setup 6 compiler (ISCC.exe) not found. Install Inno Setup 6 or add its folder to PATH",
    )?;
    let script = crate_root()
        .join("packaging")
        .join("windows")
        .join("opendefocus.iss");
    let out_dir = output.parent().context("Installer path has no parent")?;
    let stem = output
        .file_stem()
        .and_then(|stem| stem.to_str())
        .context("Installer name must be UTF-8")?;
    let args = iscc_args(&script, bundle, out_dir, stem, env!("CARGO_PKG_VERSION"));
    cmd(iscc.as_os_str(), args).run()?;
    Ok(())
}

/// Arguments for `ISCC.exe`: quiet, version and bundle path as preprocessor defines,
/// output directory and base file name, then the script.
fn iscc_args(
    script: &Path,
    bundle: &Path,
    out_dir: &Path,
    base_name: &str,
    version: &str,
) -> Vec<String> {
    vec![
        "/Q".to_owned(),
        format!("/DAppVersion={version}"),
        format!("/DBundlePath={}", bundle.display()),
        format!("/O{}", out_dir.display()),
        format!("/F{base_name}"),
        script.display().to_string(),
    ]
}

/// Finds `ISCC.exe` on PATH or in the default Inno Setup 6 install folders.
fn find_iscc() -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .flat_map(|dir| [dir.join("ISCC.exe"), dir.join("iscc.exe")])
                .collect()
        })
        .unwrap_or_default();
    for variable in ["ProgramFiles(x86)", "ProgramFiles"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(root).join("Inno Setup 6").join("ISCC.exe"));
        }
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Recursively copies the directory `from` to `to`, creating `to` and its parents. Fails
/// when a source directory cannot be read or a file cannot be copied.
fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from).with_context(|| format!("Reading {}", from.display()))? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)
                .with_context(|| format!("Copying {}", entry.path().display()))?;
        }
    }
    Ok(())
}

/// Tests for installer names and the `ISCC.exe` argument list.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the installer file names for macOS, Linux and Windows, and that unsupported
    /// platforms are rejected.
    #[test]
    fn installer_names_follow_release_convention() {
        assert_eq!(
            installer_name("macos", "aarch64", "0.1.10").unwrap(),
            "OpenDefocus-OFX-0.1.10-macos-arm64.pkg"
        );
        assert_eq!(
            installer_name("macos", "x86_64", "0.1.10").unwrap(),
            "OpenDefocus-OFX-0.1.10-macos-x86_64.pkg"
        );
        assert_eq!(
            installer_name("linux", "aarch64", "0.1.10").unwrap(),
            "OpenDefocus-OFX-0.1.10-linux-aarch64.tar.gz"
        );
        assert_eq!(
            installer_name("windows", "x86_64", "0.1.10").unwrap(),
            "OpenDefocus-OFX-0.1.10-windows-x86_64-setup.exe"
        );
        assert!(installer_name("freebsd", "x86_64", "0.1.10").is_err());
    }

    /// Verifies that the Windows setup base name drops the `.exe` extension.
    #[test]
    fn windows_setup_base_name_drops_exe_extension() {
        let name = installer_name("windows", "x86_64", "0.1.10").unwrap();
        let stem = Path::new(&name).file_stem().unwrap().to_str().unwrap();
        assert_eq!(stem, "OpenDefocus-OFX-0.1.10-windows-x86_64-setup");
    }

    /// Verifies that the `ISCC.exe` argument list carries the version, bundle path, output
    /// directory and base name in that order.
    #[test]
    fn iscc_arguments_pass_version_bundle_and_output() {
        let args = iscc_args(
            Path::new("/repo/packaging/windows/opendefocus.iss"),
            Path::new("/repo/target/ofx/OpenDefocus.ofx.bundle"),
            Path::new("/repo/target/ofx-installers"),
            "OpenDefocus-OFX-0.1.10-windows-x86_64-setup",
            "0.1.10",
        );
        assert_eq!(
            args,
            [
                "/Q",
                "/DAppVersion=0.1.10",
                "/DBundlePath=/repo/target/ofx/OpenDefocus.ofx.bundle",
                "/O/repo/target/ofx-installers",
                "/FOpenDefocus-OFX-0.1.10-windows-x86_64-setup",
                "/repo/packaging/windows/opendefocus.iss",
            ]
        );
    }
}
