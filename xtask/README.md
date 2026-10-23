# OpenDefocus xtask

This package is a cli to handle repository tasks. It is not meant to be published. Just the central part for coordination.

Its just following the xtask convention: https://github.com/matklad/cargo-xtask.

That includes:
* Creating builds (in a single unified interface)
* Testing packages
* Building docs
* Simple cross platform scripts (no bash/sh/powershell stuff!! (pls) makes cross platform handling so much better)

Build, download, and release failures return errors. Keep small regressions next
to any changed argument handling, installer parsing, or package-writing logic.
Build the standalone evaluation CLI with `cargo xtask --compile --cli`. See [CLI usage](../crates/opendefocus-cli/README.md).

Build the static WebAssembly viewer with `cargo xtask --compile --web`. Install
`wasm32-unknown-unknown` and `wasm-bindgen-cli 0.2.129` first. The complete bundle
is written to Cargo's `target/web/`; see [browser usage](../crates/opendefocus-ui/README.md#browser--webassembly).

Rust tests run
the complete workspace in release mode with the checked-in lockfile, even when
xtask is invoked from a crate directory. Incomplete build commands return usage errors instead of silently succeeding.

Documentation tasks require `cargo-about`,
`mdbook`, and `mdbook-toc`.

Compare the OFX plugin with the CLI in Natron with `cargo xtask --test-natron`. It
builds the bundle, renders `test/natron/render_occlusion.py` headlessly with
`NatronRenderer` (found via `NATRON_RENDERER`, `~/Applications` or `/Applications` on
macOS, then `PATH`; 300 s timeout), and compares the result with the CLI using idiff
at 1e-6. Outputs go to `target/natron-test/`. Hosted runners may have no GPU; the manual
workflow `.github/workflows/test-natron.yaml` runs it on demand only.

## Before/after benchmarks

```sh
# Before development: builds the release CLI and saves a baseline.
cargo xtask bench before -- test/images/scenes/focus_ramp/color.exr --radius 4
# After your changes: rebuilds and reuses the baseline's exact render options.
cargo xtask bench after

# Keep separate cases, including hardware GPU and striped rendering.
cargo xtask bench before --name gpu-depth -- color.exr --depth depth.exr --render-mode striped --stripe-height 64
cargo xtask bench after --name gpu-depth
```

Each run performs three warm-up renders and ten measured renders, restoring
pristine input each time. Comparisons print median/max frame and renderer-call
times, percentage changes, and a reminder to investigate slowdowns of at least
5%. Repeat paired runs to distinguish regressions from noise. Keep other builds
and rendering workloads idle. GPU requests fail rather than falling back to CPU.
On POSIX hosts the benchmark uses `/tmp/opendefocus-validation.lock`, the same
advisory lock used for other builds and GPU checks in [AGENTS.md](../AGENTS.md).

Raw JSON reports and final EXR images live in `benchmark-results/<name>/`
(default name: `default`). The entire directory is gitignored, and the command
refuses to run if that ignore rule is missing. A new `before` replaces that case's
baseline. The tool records revision/dirty status, toolchain, hostname, render
settings and Git blob hashes of all inputs. Changed inputs, environment, backend
or effective settings invalidate comparisons; capture a new baseline deliberately.
Outputs are compared by file hash; changed pixels require separate correctness
validation. This is wall-clock rendering measurement, not a memory profiler or
GPU kernel timer. Cold/initialization/file-I/O timings remain in the raw reports.

Pass the image first after `--`, followed by existing CLI render options. The
benchmark owns `--output`, `--report`, and `--repeat`; do not pass those yourself.
See [CLI usage](../crates/opendefocus-cli/README.md) for render options.
