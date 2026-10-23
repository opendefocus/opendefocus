//! Builds the OFX installer of the host platform, installs it into a scratch location, checks
//! and loads the installed plugin library, and uninstalls it again.
use crate::package::package_ofx;
use anyhow::{Context, Result, bail, ensure};
use duct::cmd;
use std::{
    ffi::c_int,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

/// Folder name of the OpenFX bundle inside the installer payload.
const BUNDLE_NAME: &str = "OpenDefocus.ofx.bundle";
/// File name of the plugin library inside the bundle's architecture folder.
const PLUGIN_BINARY: &str = "OpenDefocus.ofx";
/// Icon the hosts show for the plugin, named after the plugin identifier.
const PLUGIN_ICON: &str = "org.opendefocus.OpenDefocus.png";
/// Smallest size of a real plugin library; an empty or truncated file is a broken package.
const MINIMUM_BINARY_BYTES: u64 = 1_000_000;
/// Folder the macOS package must install into.
const MACOS_INSTALL_LOCATION: &str = "/Library/OFX/Plugins";
/// How long the Windows uninstaller may take to remove the bundle; it finishes in the background.
const UNINSTALL_TIMEOUT: Duration = Duration::from_secs(60);
/// Pause between checks while waiting for the uninstaller.
const POLL_INTERVAL: Duration = Duration::from_millis(500);
/// Number of plugins the library exposes through `OfxGetNumberOfPlugins`.
const EXPECTED_PLUGIN_COUNT: c_int = 1;

/// Builds the installer for the host, installs it into a temporary folder, verifies the
/// installed bundle, loads its library and removes it again.
///
/// # Errors
/// Fails when the build or the installer fails, the installed bundle is incomplete, the
/// library does not load, or the uninstaller leaves the bundle behind.
pub fn test_installer() -> Result<()> {
    let installer = package_ofx()?;
    let scratch = tempfile::tempdir().context("Creating the scratch folder")?;
    let bundle = install(&installer, scratch.path())?;
    check_bundle(&bundle)?;
    load_plugin(&bundle)?;
    uninstall(scratch.path(), &bundle)?;
    log::info!("Installer test passed for {}", installer.display());
    Ok(())
}

/// Installs `installer` below `scratch` and returns where the bundle ends up. macOS packages
/// are expanded instead of installed, because installing needs administrator rights and
/// changes the machine.
///
/// # Errors
/// Fails when the platform has no installer or the installer exits with an error.
fn install(installer: &Path, scratch: &Path) -> Result<PathBuf> {
    match std::env::consts::OS {
        "linux" => install_linux(installer, scratch),
        "windows" => install_windows(installer, scratch),
        "macos" => expand_macos(installer, scratch),
        other => bail!("No installer test for {other}"),
    }
}

/// Extracts the archive and runs its `install.sh` twice into a scratch prefix, the second
/// time to cover the replacement of an existing install.
///
/// # Errors
/// Fails when extraction or the script fails.
fn install_linux(archive: &Path, scratch: &Path) -> Result<PathBuf> {
    let extracted = scratch.join("extracted");
    std::fs::create_dir_all(&extracted)?;
    cmd!("tar", "-xzf", archive, "-C", &extracted).run()?;
    let prefix = scratch.join("prefix");
    for _ in 0..2 {
        cmd!("sh", extracted.join("install.sh"), "--prefix", &prefix).run()?;
    }
    Ok(prefix.join("OFX").join("Plugins").join(BUNDLE_NAME))
}

/// Runs the Inno Setup installer silently into a scratch folder.
///
/// # Errors
/// Fails when the installer exits with an error.
fn install_windows(setup: &Path, scratch: &Path) -> Result<PathBuf> {
    let plugins = scratch.join("Plugins");
    cmd(
        setup.as_os_str(),
        [
            "/VERYSILENT".into(),
            "/SUPPRESSMSGBOXES".into(),
            "/NORESTART".into(),
            "/SP-".into(),
            format!("/DIR={}", plugins.display()),
        ],
    )
    .run()?;
    Ok(plugins.join(BUNDLE_NAME))
}

/// Expands the product archive and checks that its component installs into the OFX folder.
/// Returns the bundle inside the expanded payload.
///
/// # Errors
/// Fails when the package cannot be expanded, has no bundle, or installs elsewhere.
fn expand_macos(package: &Path, scratch: &Path) -> Result<PathBuf> {
    let expanded = scratch.join("expanded");
    cmd!("pkgutil", "--expand-full", package, &expanded).run()?;
    let info = find_entry(&expanded, "PackageInfo").context("The package has no PackageInfo")?;
    let info = std::fs::read_to_string(info)?;
    ensure!(
        info.contains(&format!("install-location=\"{MACOS_INSTALL_LOCATION}\"")),
        "The package does not install into {MACOS_INSTALL_LOCATION}"
    );
    find_entry(&expanded, BUNDLE_NAME).context("The package payload has no bundle")
}

/// Removes the installed bundle with the platform's own mechanism and checks it is gone.
///
/// # Errors
/// Fails when the uninstaller fails or the bundle is still there afterwards.
fn uninstall(scratch: &Path, bundle: &Path) -> Result<()> {
    match std::env::consts::OS {
        "linux" => {
            let script = scratch.join("extracted").join("uninstall.sh");
            cmd!("sh", script, "--prefix", scratch.join("prefix")).run()?;
        }
        "windows" => {
            let uninstaller = scratch.join("Plugins").join("unins000.exe");
            cmd!(
                uninstaller,
                "/VERYSILENT",
                "/SUPPRESSMSGBOXES",
                "/NORESTART"
            )
            .run()?;
            wait_until_removed(bundle)?;
        }
        _ => return Ok(()),
    }
    ensure!(
        !bundle.exists(),
        "The uninstaller left {}",
        bundle.display()
    );
    Ok(())
}

/// Waits until `path` no longer exists, because the Windows uninstaller hands the work to a
/// copy of itself and returns before the files are gone.
///
/// # Errors
/// Fails when the path still exists after `UNINSTALL_TIMEOUT`.
fn wait_until_removed(path: &Path) -> Result<()> {
    let started = Instant::now();
    while path.exists() {
        ensure!(
            started.elapsed() < UNINSTALL_TIMEOUT,
            "{} was not removed within {} s",
            path.display(),
            UNINSTALL_TIMEOUT.as_secs()
        );
        std::thread::sleep(POLL_INTERVAL);
    }
    Ok(())
}

/// Finds the first file or folder called `name` below `root`.
fn find_entry(root: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(root).ok()?.flatten() {
        let path = entry.path();
        if entry.file_name() == name {
            return Some(path);
        }
        if path.is_dir()
            && let Some(found) = find_entry(&path, name)
        {
            return Some(found);
        }
    }
    None
}

/// Returns the path of the plugin library inside `bundle`: the only file with the plugin
/// name below `Contents`, whatever the architecture folder is called.
///
/// # Errors
/// Fails when the library is missing.
fn plugin_binary(bundle: &Path) -> Result<PathBuf> {
    let contents = bundle.join("Contents");
    find_entry(&contents, PLUGIN_BINARY)
        .with_context(|| format!("No {PLUGIN_BINARY} below {}", contents.display()))
}

/// Verifies that the installed bundle holds a plausible plugin library and the icon.
///
/// # Errors
/// Fails when the library is missing or too small, or a file the bundle needs is absent.
fn check_bundle(bundle: &Path) -> Result<()> {
    ensure!(bundle.is_dir(), "{} was not installed", bundle.display());
    let binary = plugin_binary(bundle)?;
    let size = std::fs::metadata(&binary)?.len();
    ensure!(
        size >= MINIMUM_BINARY_BYTES,
        "{} is only {size} bytes",
        binary.display()
    );
    let icon = bundle.join("Contents").join("Resources").join(PLUGIN_ICON);
    ensure!(icon.is_file(), "The bundle has no {PLUGIN_ICON}");
    if std::env::consts::OS == "macos" {
        let plist = bundle.join("Contents").join("Info.plist");
        ensure!(plist.is_file(), "The bundle has no Info.plist");
    }
    Ok(())
}

/// Loads the installed library and calls `OfxGetNumberOfPlugins`, which is what a host does
/// first. This catches a library built for another platform or missing a dependency.
///
/// # Errors
/// Fails when the library does not load, lacks the entry point, or reports another count.
fn load_plugin(bundle: &Path) -> Result<()> {
    let binary = plugin_binary(bundle)?;
    // SAFETY: loading runs the library's initializers; this is the OpenDefocus plugin that
    // was just built by this workflow, and the entry point has the OpenFX signature.
    let count = unsafe {
        let library = libloading::Library::new(&binary)
            .with_context(|| format!("Loading {}", binary.display()))?;
        let entry: libloading::Symbol<unsafe extern "C" fn() -> c_int> = library
            .get(b"OfxGetNumberOfPlugins\0")
            .context("The library has no OfxGetNumberOfPlugins")?;
        entry()
    };
    ensure!(
        count == EXPECTED_PLUGIN_COUNT,
        "The library reports {count} plugins instead of {EXPECTED_PLUGIN_COUNT}"
    );
    Ok(())
}

/// Tests for the bundle checks.
#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a bundle with a library of `size` bytes and, optionally, the icon.
    fn bundle_with(root: &Path, size: usize, icon: bool) -> PathBuf {
        let bundle = root.join(BUNDLE_NAME);
        let binary_dir = bundle.join("Contents").join("Arch");
        std::fs::create_dir_all(&binary_dir).unwrap();
        std::fs::write(binary_dir.join(PLUGIN_BINARY), vec![0u8; size]).unwrap();
        let resources = bundle.join("Contents").join("Resources");
        std::fs::create_dir_all(&resources).unwrap();
        if icon {
            std::fs::write(resources.join(PLUGIN_ICON), b"png").unwrap();
        }
        bundle
    }

    /// Verifies that a complete bundle passes the check on Linux and Windows.
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn test_complete_bundle_passes() {
        let root = tempfile::tempdir().unwrap();
        let bundle = bundle_with(root.path(), MINIMUM_BINARY_BYTES as usize, true);
        assert!(check_bundle(&bundle).is_ok());
    }

    /// Verifies that a tiny library and a missing icon are rejected.
    #[test]
    fn test_broken_bundles_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let tiny = bundle_with(&root.path().join("a"), 10, true);
        assert!(check_bundle(&tiny).is_err());
        let no_icon = bundle_with(&root.path().join("b"), MINIMUM_BINARY_BYTES as usize, false);
        assert!(check_bundle(&no_icon).is_err());
        assert!(check_bundle(&root.path().join("missing")).is_err());
    }

    /// Verifies that entries are found at any depth and a missing one is reported.
    #[test]
    fn test_find_entry_searches_nested_folders() {
        let root = tempfile::tempdir().unwrap();
        let deep = root.path().join("a").join("b");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("target.txt"), b"x").unwrap();
        assert_eq!(
            find_entry(root.path(), "target.txt"),
            Some(deep.join("target.txt"))
        );
        assert_eq!(find_entry(root.path(), "other.txt"), None);
    }

    /// Verifies that waiting for a path that is already gone returns at once.
    #[test]
    fn test_wait_until_removed_returns_for_missing_path() {
        let root = tempfile::tempdir().unwrap();
        assert!(wait_until_removed(&root.path().join("gone")).is_ok());
    }
}
