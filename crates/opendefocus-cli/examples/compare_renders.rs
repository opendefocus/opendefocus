//! Re-renders the parity command list with a CLI binary and compares the float EXRs
//! with a baseline. The command list and settings are in `test/parity`; `cargo xtask parity` renders the baseline with `--render`.
//!
//! ```sh
//! cargo run --release -p opendefocus-cli --example compare_renders -- \
//!   --parity DIR --cli target/release/opendefocus-cli --out DIR2 [--repo .]
//! ```
//!
//! `DIR/commands.tsv` has one `name<TAB>arguments` line per render; `{repo}` and `{parity}`
//! are substituted. `DIR/exr/<name>.exr` is the baseline. Use `--render` to create the
//! baseline itself (written to `--out`). Exit status is 1 when any file exceeds `--tolerance`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly with a panic; only #[test] functions are exempt from these lints by clippy.toml"
)]

use anyhow::{Context, Result, bail, ensure};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Parsed command-line options for a parity comparison run.
struct Options {
    /// Directory holding `commands.tsv` and the baseline `exr/` files.
    parity: PathBuf,
    /// CLI binary that renders each command.
    cli: PathBuf,
    /// Directory that receives the freshly rendered EXRs.
    out: PathBuf,
    /// Repository root substituted for `{repo}` in the command list.
    repo: PathBuf,
    /// Largest allowed absolute per-channel difference before a file fails.
    tolerance: f32,
    /// Render without comparing, to create a baseline.
    render_only: bool,
}

/// Parses `--parity`, `--cli`, `--out`, `--repo`, `--tolerance` and `--render` from the command line.
///
/// # Errors
///
/// Returns an error for an unknown flag, a missing value, or a required path that is missing.
fn parse() -> Result<Options> {
    let mut args = std::env::args().skip(1);
    let (mut parity, mut cli, mut out, mut repo) = (None, None, None, PathBuf::from("."));
    let (mut tolerance, mut render_only) = (1e-6, false);
    while let Some(arg) = args.next() {
        let mut value = || args.next().with_context(|| format!("{arg} needs a value"));
        match arg.as_str() {
            "--parity" => parity = Some(PathBuf::from(value()?)),
            "--cli" => cli = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--repo" => repo = PathBuf::from(value()?),
            "--tolerance" => tolerance = value()?.parse()?,
            "--render" => render_only = true,
            other => bail!("Unknown argument {other}"),
        }
    }
    Ok(Options {
        parity: parity.context("--parity is required")?,
        cli: cli.context("--cli is required")?,
        out: out.context("--out is required")?,
        repo: repo.canonicalize().context("--repo")?,
        tolerance,
        render_only,
    })
}

/// Reads an image as RGBA float and returns its width, height and interleaved values.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or decoded.
fn load(path: &Path) -> Result<(u32, u32, Vec<f32>)> {
    let image = image::open(path)
        .with_context(|| format!("Reading {}", path.display()))?
        .into_rgba32f();
    Ok((image.width(), image.height(), image.into_raw()))
}

/// Renders every command, then compares each output with its baseline and prints per-file statistics.
///
/// Exits with status 1 when a file differs by more than the tolerance or changes size.
///
/// # Errors
///
/// Returns an error if a render fails or a file cannot be read.
fn main() -> Result<()> {
    let options = parse()?;
    let parity = options.parity.canonicalize()?;
    let commands = std::fs::read_to_string(parity.join("commands.tsv"))?;
    std::fs::create_dir_all(&options.out)?;
    let out = options.out.canonicalize()?;
    println!(
        "{:<34} {:>12} {:>12} {:>9} {:>9} {:>8}  status",
        "file", "max abs", "mean abs", "a min", "a max", "bitdiff"
    );
    let mut failures = 0;
    let mut worst = 0.0f32;
    for line in commands.lines().filter(|line| !line.trim().is_empty()) {
        let (name, rest) = line.split_once('\t').context("Expected name<TAB>args")?;
        let rest = rest
            .replace("{repo}", options.repo.to_str().unwrap())
            .replace("{parity}", parity.to_str().unwrap());
        let output = out.join(format!("{name}.exr"));
        let status = Command::new(&options.cli)
            .args(rest.split_whitespace())
            .arg("--output")
            .arg(&output)
            .stdout(std::process::Stdio::null())
            .status()?;
        ensure!(status.success(), "Render failed for {name}");
        if options.render_only {
            println!("rendered {name}");
            continue;
        }
        let (bw, bh, base) = load(&parity.join("exr").join(format!("{name}.exr")))?;
        let (w, h, new) = load(&output)?;
        if (bw, bh) != (w, h) {
            println!("{name:<34} size mismatch {bw}x{bh} vs {w}x{h}");
            failures += 1;
            continue;
        }
        let (mut max, mut sum) = (0.0f32, 0.0f64);
        let (mut a_min, mut a_max) = (f32::MAX, f32::MIN);
        let (mut finite, mut bit_diffs) = (true, 0usize);
        for (index, (a, b)) in base.iter().zip(&new).enumerate() {
            finite &= b.is_finite();
            let diff = if a.is_finite() && b.is_finite() {
                (a - b).abs()
            } else if a.to_bits() == b.to_bits() {
                0.0
            } else {
                f32::INFINITY
            };
            bit_diffs += usize::from(a.to_bits() != b.to_bits());
            max = max.max(diff);
            sum += diff as f64;
            if index % 4 == 3 {
                a_min = a_min.min(*b);
                a_max = a_max.max(*b);
            }
        }
        let ok = finite && max <= options.tolerance;
        failures += usize::from(!ok);
        worst = worst.max(max);
        println!(
            "{name:<34} {max:>12.3e} {:>12.3e} {a_min:>9.5} {a_max:>9.5} {bit_diffs:>8}  {}",
            sum / base.len() as f64,
            if ok { "ok" } else { "FAIL" }
        );
    }
    if !options.render_only {
        println!(
            "worst max abs diff {worst:.3e}, tolerance {:e}, {failures} failing",
            options.tolerance
        );
        if failures > 0 {
            std::process::exit(1);
        }
    }
    Ok(())
}
