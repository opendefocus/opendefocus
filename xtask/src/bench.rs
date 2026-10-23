//! Benchmarks the release CLI renderer. The `before` phase records a baseline; the `after`
//! phase renders again and compares the timings with that baseline.
use crate::util::{crate_root, target_directory, validation_lock};
use anyhow::{Context, Result, ensure};
use duct::cmd;
use serde_json::{Value, json};
use std::{ffi::OsString, fs, path::Path};

/// Frames rendered and discarded before timing starts.
const WARMUP: usize = 3;
/// Measured frames per phase; the statistics are computed from these.
const SAMPLES: usize = 10;

/// Benchmark phase: `Before` records the baseline and `After` compares a new run with it.
#[derive(clap::ValueEnum, Clone, Copy, Debug)]
pub enum Phase {
    /// Records the baseline run.
    Before,
    /// Compares a new run with the saved baseline.
    After,
}

/// Arguments of `cargo xtask bench`: the phase, the name of the workload whose baseline is
/// used, and the render arguments for the baseline.
#[derive(clap::Args, Debug)]
pub struct Args {
    /// Phase to run: `before` records a baseline, `after` compares with it.
    #[arg(value_enum)]
    phase: Phase,
    /// Separate baseline for each workload (letters, numbers, hyphens, underscores)
    #[arg(long, default_value = "default")]
    name: String,
    /// Before: input image followed by CLI render options. After: reuses saved options
    #[arg(last = true, value_name = "RENDER_ARGS")]
    render: Vec<String>,
}

/// Reads and parses the JSON file at `path`. Fails with a hint to run `bench before` first
/// when the file is missing.
fn read(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path).with_context(
        || format!("Reading {}; run bench before first", path.display()),
    )?)?)
}

/// Returns the Git blob hash of `path`, computed with `--no-filters` so that Git filters
/// do not change the result.
fn hash_file(path: &Path) -> Result<String> {
    Ok(cmd!("git", "hash-object", "--no-filters", path).read()?)
}

/// Lists the inputs of a render as JSON, each with its Git blob hash. The first argument is
/// the input image; the files named by `--depth`, `--bokeh` and `--settings` are added
/// after it. Fails when the first argument is an option, an option value is missing, or an
/// argument would override output, report, repeat or settings-dump options that the
/// benchmark owns.
fn input_hashes(args: &[String], root: &Path) -> Result<Value> {
    ensure!(
        args.first().is_some_and(|arg| !arg.starts_with('-')),
        "Pass an input image first, after --"
    );
    let mut paths = vec![args[0].clone()];
    for (i, arg) in args.iter().enumerate() {
        let flag = arg.split('=').next().unwrap_or(arg);
        ensure!(
            ![
                "--output",
                "--report",
                "--repeat",
                "--dump-settings",
                "-o",
                "--"
            ]
            .contains(&flag)
                && !arg.starts_with("-o"),
            "Benchmark owns output, report and repeat options"
        );
        if ["--depth", "--bokeh", "--settings"].contains(&flag) {
            paths.push(
                arg.split_once('=')
                    .map(|(_, value)| value)
                    .or_else(|| args.get(i + 1).map(String::as_str))
                    .context("Missing input option value")?
                    .to_owned(),
            );
        }
    }
    paths
        .iter()
        .map(|path| Ok(json!({"path": path, "git_blob_hash": hash_file(&root.join(path))?})))
        .collect()
}

/// Checks that `after` can be compared with `before`: the environment, backend, device,
/// image size, stripe height and settings must match. Fails at the first difference.
fn compatible(before: &Value, after: &Value) -> Result<()> {
    ensure!(
        before["environment"] == after["environment"],
        "Machine, toolchain, build flags or input contents changed; capture a new baseline"
    );
    for key in [
        "backend",
        "device",
        "width",
        "height",
        "stripe_height",
        "settings",
    ] {
        ensure!(
            before["report"][key] == after["report"][key],
            "Benchmark {key} changed; capture a new baseline"
        );
    }
    Ok(())
}

/// Returns the median and the maximum of the measured frame timings under `key`, skipping
/// warm-up frames. Fails when the report does not contain exactly the expected number of
/// frames or a sample is not a positive finite number.
fn timings(report: &Value, key: &str) -> Result<(f64, f64)> {
    let frames = report["frames"]
        .as_array()
        .context("Missing frame timings")?;
    ensure!(
        frames.len() == WARMUP + SAMPLES,
        "Expected 3 warm-ups and 10 measured frames"
    );
    let mut times = frames[WARMUP..]
        .iter()
        .map(|frame| {
            frame[key]
                .as_f64()
                .filter(|v| v.is_finite() && *v > 0.0)
                .context("Invalid frame timing")
        })
        .collect::<Result<Vec<_>>>()?;
    times.sort_by(f64::total_cmp);
    Ok((f64::midpoint(times[4], times[5]), times[9]))
}

/// Runs one benchmark phase. It checks the workload name and that the output folder is
/// gitignored, renders with the release CLI, and writes `<phase>.json` and `<phase>.exr`.
/// For `after` it also prints the comparison with the baseline. Fails when a precondition
/// is not met, the baseline is missing or incompatible, or the render fails.
pub fn run(args: Args) -> Result<()> {
    ensure!(
        !args.name.is_empty()
            && args
                .name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
        "Name must contain only letters, numbers, hyphens or underscores"
    );
    let root = crate_root().canonicalize()?;
    let directory = root.join("benchmark-results").join(&args.name);
    for file in ["before.json", "after.json", "before.exr", "after.exr"] {
        ensure!(
            cmd!("git", "check-ignore", "--quiet", directory.join(file))
                .dir(&root)
                .unchecked()
                .run()?
                .status
                .success(),
            "benchmark-results/ must be gitignored before benchmarking"
        );
    }
    ensure!(
        cmd!("git", "ls-files", "--", "benchmark-results")
            .dir(&root)
            .read()?
            .is_empty(),
        "Remove tracked benchmark artifacts from Git before benchmarking"
    );
    fs::create_dir_all(&directory)?;
    let _lock = validation_lock()?;
    let before_path = directory.join("before.json");
    let before = if matches!(args.phase, Phase::After) {
        Some(read(&before_path)?)
    } else {
        None
    };
    ensure!(
        before.is_none() || args.render.is_empty(),
        "After reuses the baseline's render options; omit arguments after --"
    );
    let render_args: Vec<String> = match &before {
        Some(value) => serde_json::from_value(value["args"].clone())?,
        None => args.render,
    };
    let environment = json!({
        "host": cmd!("hostname").read()?, "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
        "threads": std::thread::available_parallelism()?.get(),
        "rustc": cmd!("rustc", "-Vv").read()?,
        "rustflags": std::env::var("RUSTFLAGS").ok(),
        "encoded_rustflags": std::env::var("CARGO_ENCODED_RUSTFLAGS").ok(),
        "rayon_threads": std::env::var("RAYON_NUM_THREADS").ok(),
        "inputs": input_hashes(&render_args, &root)?,
    });
    if let Some(before) = &before {
        ensure!(
            before["environment"] == environment,
            "Machine, toolchain, build flags or input contents changed; capture a new baseline"
        );
    }
    let revision = cmd!("git", "rev-parse", "HEAD").dir(&root).read()?;
    let dirty = !cmd!("git", "status", "--porcelain", "--untracked-files=no")
        .dir(&root)
        .read()?
        .is_empty();
    let phase = if before.is_some() { "after" } else { "before" };
    let build = vec!["build", "--release", "--locked", "-p", "opendefocus-cli"];
    cmd("cargo", build).dir(&root).run()?;
    println!("Running {phase}: 3 warm-ups + 10 measured renders. Keep other workloads idle.");
    let output = directory.join(format!("{phase}.exr"));
    let executable = target_directory()
        .join("release")
        .join(format!("opendefocus-cli{}", std::env::consts::EXE_SUFFIX));
    let mut command_args: Vec<OsString> = render_args.iter().map(OsString::from).collect();
    command_args.extend(["--repeat", "13", "--output"].map(OsString::from));
    command_args.push(output.clone().into_os_string());
    let report: Value = serde_json::from_str(&cmd(executable, command_args).dir(&root).read()?)?;
    for key in ["frame_ms", "renderer_calls_ms"] {
        timings(&report, key)?;
    }
    let result = json!({"revision": revision, "dirty": dirty, "args": render_args, "environment": environment, "report": report, "output_hash": hash_file(&output)?});
    fs::write(
        directory.join(format!("{phase}.json")),
        serde_json::to_vec_pretty(&result)?,
    )?;
    println!("Reports and output images: {}", directory.display());
    if dirty {
        println!("Working tree has tracked changes; report marked dirty.");
    }
    if let Some(before) = before {
        compatible(&before, &result)?;
        println!(
            "Before {} → after {}",
            before["revision"], result["revision"]
        );
        for key in ["renderer_calls_ms", "frame_ms"] {
            let (old, old_max) = timings(&before["report"], key)?;
            let (new, new_max) = timings(&result["report"], key)?;
            let change = (new / old - 1.0) * 100.0;
            println!(
                "{key}: median {old:.2} → {new:.2} ms ({change:+.1}%); max {old_max:.2} → {new_max:.2} ms"
            );
            if change >= 5.0 {
                println!("Slowdown ≥5%: repeat paired runs and investigate before merging.");
            }
        }
        println!(
            "Output bytes {}. Validate changed pixels separately.",
            if before["output_hash"] == result["output_hash"] {
                "match"
            } else {
                "differ"
            }
        );
    }
    println!(
        "Cold first frame, initialization and end-to-end timings are in the raw report; warm timings exclude file I/O. Memory is not measured."
    );
    Ok(())
}

/// Tests for benchmark statistics and argument validation.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that warm-up frames are excluded, the median is taken, and invalid samples are
    /// rejected.
    #[test]
    fn test_statistics_exclude_warmup_and_reject_invalid_samples() {
        let mut frames = vec![json!({"frame_ms": 999.0}); WARMUP];
        frames.extend((1..=10).rev().map(|n| json!({"frame_ms": n})));
        let mut report = json!({"frames": frames});
        assert_eq!(timings(&report, "frame_ms").unwrap(), (5.5, 10.0));
        report["frames"][3]["frame_ms"] = json!(-1);
        assert!(timings(&report, "frame_ms").is_err());
        assert!(timings(&json!({"frames": []}), "frame_ms").is_err());
    }

    /// Verifies that options overriding the benchmark's output, report or repeat settings are
    /// rejected.
    #[test]
    fn test_output_options_cannot_override_ignored_artifacts() {
        for option in [
            "--output=outside.exr",
            "-ooutside.exr",
            "--report=outside.json",
            "--repeat=1",
            "--dump-settings",
        ] {
            let args = vec!["input.exr".to_owned(), option.to_owned()];
            assert!(
                input_hashes(&args, Path::new("."))
                    .unwrap_err()
                    .to_string()
                    .contains("Benchmark owns")
            );
        }
    }

    /// Verifies that a change in backend or inputs makes the comparison fail.
    #[test]
    fn test_comparison_rejects_changed_inputs_and_backends() {
        let before = json!({"environment": {"inputs": ["hash"]}, "report": {"backend": "gpu"}});
        assert!(compatible(&before, &before).is_ok());
        let mut after = before.clone();
        after["report"]["backend"] = json!("cpu");
        assert!(compatible(&before, &after).is_err());
        after = before.clone();
        after["environment"]["inputs"] = json!(["other"]);
        assert!(compatible(&before, &after).is_err());
    }
}
