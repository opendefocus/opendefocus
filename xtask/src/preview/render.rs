//! Runs one CLI build on one workload and reads back the warm frame timing.
use super::workload::Workload;
use anyhow::{Context, Result, ensure};
use duct::cmd;
use serde_json::Value;
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

/// Frames rendered and discarded before timing starts.
pub const WARMUP: usize = 2;

/// What one CLI run produced.
#[derive(Debug)]
pub struct Render {
    /// Float result, used for the pixel comparison.
    pub exr: PathBuf,
    /// Median warm frame time in milliseconds.
    pub median_ms: f64,
    /// Name of the adapter the render ran on.
    pub device: String,
}

/// Returns the median of `values`, or `None` when it is empty.
fn median(mut values: Vec<f64>) -> Option<f64> {
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;
    match values.len() {
        0 => None,
        length if length % 2 == 1 => Some(values[middle]),
        _ => Some(f64::midpoint(values[middle - 1], values[middle])),
    }
}

/// Reads the median `frame_ms` of the frames after the warm-up from a CLI report.
///
/// # Errors
/// Fails when the report has no timed frames or a timing is not a finite positive number.
fn warm_median(report: &Value) -> Result<f64> {
    let frames = report["frames"]
        .as_array()
        .context("Report has no frames")?;
    let times = frames
        .iter()
        .skip(WARMUP)
        .map(|frame| {
            frame["frame_ms"]
                .as_f64()
                .filter(|ms| ms.is_finite() && *ms > 0.0)
                .context("Invalid frame timing")
        })
        .collect::<Result<Vec<_>>>()?;
    median(times).context("Report has no timed frames")
}

/// Renders `workload` with the CLI at `cli` into `folder`.
///
/// # Errors
/// Fails when the CLI exits with an error, or its report cannot be read.
pub fn render(cli: &Path, workload: &Workload, folder: &Path, frames: usize) -> Result<Render> {
    fs::create_dir_all(folder)?;
    let stem = &workload.name;
    let exr = folder.join(format!("{stem}.exr"));
    let png = folder.join(format!("{stem}.png"));
    let report_path = folder.join(format!("{stem}.json"));
    let mut arguments: Vec<OsString> = workload.args.iter().map(OsString::from).collect();
    arguments.extend(["--repeat".into(), frames.to_string().into()]);
    for (flag, path) in [
        ("--output", &exr),
        ("--preview", &png),
        ("--report", &report_path),
    ] {
        arguments.extend([flag.into(), path.clone().into_os_string()]);
    }
    cmd(cli, arguments)
        .stdout_null()
        .run()
        .with_context(|| format!("Rendering {stem} with {}", cli.display()))?;
    let report: Value = serde_json::from_slice(&fs::read(&report_path)?)?;
    ensure!(
        exr.is_file() && png.is_file(),
        "The CLI wrote no image for {stem}"
    );
    Ok(Render {
        exr,
        median_ms: warm_median(&report)?,
        device: report["device"].as_str().unwrap_or("unknown").to_owned(),
    })
}

/// Tests for the timing statistics.
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Verifies the median of odd, even and empty inputs.
    #[test]
    fn test_median() {
        assert_eq!(median(vec![3.0, 1.0, 2.0]), Some(2.0));
        assert_eq!(median(vec![4.0, 1.0, 2.0, 3.0]), Some(2.5));
        assert_eq!(median(vec![]), None);
    }

    /// Verifies that warm-up frames are skipped and bad timings are rejected.
    #[test]
    fn test_warm_median_skips_warmup_and_rejects_bad_samples() {
        let report = json!({"frames": [
            {"frame_ms": 900.0}, {"frame_ms": 900.0},
            {"frame_ms": 10.0}, {"frame_ms": 30.0}, {"frame_ms": 20.0},
        ]});
        assert_eq!(warm_median(&report).unwrap(), 20.0);
        let bad = json!({"frames": [{"frame_ms": 1.0}, {"frame_ms": 1.0}, {"frame_ms": -1.0}]});
        assert!(warm_median(&bad).is_err());
        assert!(warm_median(&json!({"frames": [{"frame_ms": 1.0}]})).is_err());
    }
}
