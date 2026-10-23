//! Integration check that renders the occlusion scene through the OFX bundle in
//! `NatronRenderer` and through the CLI, then compares the two images.
use crate::{
    compile::compile_cli,
    release::bundle_ofx,
    util::{crate_root, target_directory, validation_lock},
};
use anyhow::{Context, Result, bail, ensure};
use duct::cmd;
use std::{
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    thread,
    time::{Duration, Instant},
};

/// Longest time a `NatronRenderer` run may take before it is killed.
const TIMEOUT: Duration = Duration::from_secs(300);
/// Plugin and CLI renders of the same scene on the same machine must match this closely.
const TOLERANCE: &str = "1e-6";
/// Occlusion scene settings, matching `test/natron/render_occlusion.py`.
const SETTINGS: &str = r#"{"defocus":{"use_direct_math":true,"camera_max_size":24.0,"circle_of_confusion":{"max_size":24.0}}}"#;

/// Natron.app locations checked before PATH; only macOS installs use app bundles.
fn natron_app_candidates(home: Option<&Path>) -> Vec<PathBuf> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let mut candidates: Vec<PathBuf> = home
        .map(|home| home.join("Applications/Natron.app/Contents/MacOS/NatronRenderer"))
        .into_iter()
        .collect();
    candidates.push("/Applications/Natron.app/Contents/MacOS/NatronRenderer".into());
    candidates
}

/// Find `NatronRenderer`: `NATRON_RENDERER`, then the app candidates, then PATH.
fn locate_natron_renderer(
    env_value: Option<OsString>,
    app_candidates: Vec<PathBuf>,
    path_value: Option<&OsStr>,
) -> Result<PathBuf> {
    if let Some(value) = env_value {
        let path = PathBuf::from(value);
        ensure!(
            path.is_file(),
            "NATRON_RENDERER points to {}, which is not a file",
            path.display()
        );
        return Ok(path);
    }
    let name = format!("NatronRenderer{}", std::env::consts::EXE_SUFFIX);
    let path_candidates = path_value
        .map(std::env::split_paths)
        .into_iter()
        .flatten()
        .map(|dir| dir.join(&name));
    app_candidates
        .into_iter()
        .chain(path_candidates)
        .find(|path| path.is_file())
        .context("NatronRenderer not found: set NATRON_RENDERER to its path or install Natron")
}

/// Wait for the child, killing it when the timeout expires.
fn wait_with_timeout(mut child: std::process::Child, timeout: Duration) -> Result<ExitStatus> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status);
        }
        if start.elapsed() >= timeout {
            child.kill()?;
            child.wait()?;
            bail!("Timed out after {} s and was killed", timeout.as_secs());
        }
        thread::sleep(Duration::from_millis(200));
    }
}

/// Render the occlusion scene through the OFX bundle in `NatronRenderer` and through the CLI,
/// then compare the two with idiff at TOLERANCE.
pub fn test_natron() -> Result<()> {
    let _lock = validation_lock()?;
    let home = std::env::var_os("HOME");
    let renderer = locate_natron_renderer(
        std::env::var_os("NATRON_RENDERER"),
        natron_app_candidates(home.as_deref().map(Path::new)),
        std::env::var_os("PATH").as_deref(),
    )?;
    println!("NatronRenderer: {}", renderer.display());
    let plugin_path = bundle_ofx()?
        .parent()
        .context("OFX bundle has no parent folder")?
        .to_path_buf();

    let out = target_directory().join("natron-test");
    fs::create_dir_all(&out)?;
    let scene = crate_root().join("test/images/scenes/occlusion");
    let plugin_exr = out.join("plugin.exr");
    let cli_exr = out.join("cli.exr");
    let log = out.join("natron.log");
    remove_file_if_present(&plugin_exr)?;

    let log_file = fs::File::create(&log)?;
    let child = Command::new(&renderer)
        .arg("-t")
        .arg(crate_root().join("test/natron/render_occlusion.py"))
        .env("OFX_PLUGIN_PATH", &plugin_path)
        .env("OPENDEFOCUS_REPO", crate_root())
        .env("OPENDEFOCUS_NATRON_OUT", &plugin_exr)
        .stdin(Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file)
        .spawn()
        .with_context(|| format!("Starting {}", renderer.display()))?;
    let status = wait_with_timeout(child, TIMEOUT)?;
    ensure!(
        status.success(),
        "NatronRenderer exited with {status}; see {}",
        log.display()
    );
    ensure!(
        plugin_exr.is_file(),
        "NatronRenderer did not write {}; see {}",
        plugin_exr.display(),
        log.display()
    );

    compile_cli()?;
    let settings = out.join("settings.json");
    fs::write(&settings, SETTINGS)?;
    let cli = target_directory()
        .join("release")
        .join(format!("opendefocus-cli{}", std::env::consts::EXE_SUFFIX));
    cmd!(
        cli,
        scene.join("color.exr"),
        "--output",
        &cli_exr,
        "--depth",
        scene.join("coc.exr"),
        "--quality",
        "fast",
        "--settings",
        &settings
    )
    .run()?;

    let idiff = renderer.with_file_name(format!("idiff{}", std::env::consts::EXE_SUFFIX));
    let compared = cmd!(
        idiff,
        "-fail",
        TOLERANCE,
        "-warn",
        TOLERANCE,
        &plugin_exr,
        &cli_exr
    )
    .stderr_to_stdout()
    .unchecked()
    .run()?;
    print!("{}", String::from_utf8_lossy(&compared.stdout));
    ensure!(
        compared.status.success(),
        "Plugin and CLI renders differ by more than {TOLERANCE} (idiff exit {})",
        compared.status
    );
    println!("PASS: OFX plugin and CLI renders agree within {TOLERANCE}");
    Ok(())
}

/// Removes the file at `path`; a file that does not exist is not an error.
fn remove_file_if_present(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}

/// Tests for the `NatronRenderer` lookup and the render timeout.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a child process that outlives the timeout is killed and reported as an
    /// error.
    #[cfg(unix)]
    #[test]
    fn test_hung_renderer_is_killed_at_timeout() -> Result<()> {
        let child = Command::new("sleep").arg("30").spawn()?;
        let started = Instant::now();
        assert!(wait_with_timeout(child, Duration::from_millis(300)).is_err());
        assert!(started.elapsed() < Duration::from_secs(20));
        Ok(())
    }

    /// Verifies removing a missing file succeeds and an existing file is deleted.
    #[test]
    fn test_remove_file_if_present() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let file = dir.path().join("out.exr");
        remove_file_if_present(&file)?;
        fs::write(&file, "")?;
        remove_file_if_present(&file)?;
        assert!(!file.exists());
        Ok(())
    }

    /// Verifies the lookup order: `NATRON_RENDERER`, then the app bundle, then `PATH`. A
    /// `NATRON_RENDERER` path that does not exist must not fall back to the other sources.
    #[test]
    fn test_natron_renderer_lookup_order() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let touch = |path: PathBuf| -> Result<PathBuf> {
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, "")?;
            Ok(path)
        };
        let app_binary = touch(dir.path().join("app/NatronRenderer"))?;
        let path_binary = touch(dir.path().join("bin/NatronRenderer"))?;
        let env_binary = touch(dir.path().join("env/NatronRenderer"))?;
        let path_value = std::env::join_paths([dir.path().join("bin")])?;
        let apps = vec![dir.path().join("app/NatronRenderer")];
        let missing = dir.path().join("missing");

        assert!(
            locate_natron_renderer(Some(missing.into_os_string()), apps.clone(), None).is_err(),
            "a missing NATRON_RENDERER must not fall back"
        );
        assert_eq!(
            locate_natron_renderer(
                Some(env_binary.clone().into_os_string()),
                apps.clone(),
                Some(&path_value)
            )?,
            env_binary
        );
        assert_eq!(
            locate_natron_renderer(None, apps, Some(&path_value))?,
            app_binary
        );
        assert_eq!(
            locate_natron_renderer(None, vec![], Some(&path_value))?,
            path_binary
        );
        assert!(locate_natron_renderer(None, vec![], None).is_err());
        Ok(())
    }
}
