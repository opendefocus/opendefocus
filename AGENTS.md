# OpenDefocus agent instructions

## Start here

These are the shared repository instructions for coding agents.
Before renderer work, read [ARCHITECTURE.md](ARCHITECTURE.md). `docs/` holds
user-facing documentation only; keep agent notes, handoffs and plans out of it.
Read source before relying on a historical result. Explicit instructions from the user take precedence.

## Where knowledge goes

Document what a future reader cannot recover from the code, in exactly one place:

| Kind of knowledge | Home |
| --- | --- |
| How to work in this repo: rules, commands, gates, decisions, known gaps | `AGENTS.md` (keep it short; edit entries, do not append dated logs) |
| Why the renderer is built this way: invariants, buffer layouts, failure modes, measured lessons | `ARCHITECTURE.md` |
| Crate usage and options | that crate's `README.md` |
| End-user and developer guides | `docs/src/` (mdbook only) |
| Results of a task, validation run, benchmark, next steps | the PR description or commit message |

Rules:

- `README.md` is marketing and usage text for users. Keep test status, host gaps,
  file-format internals and "not yet tested" caveats out of it; they belong in
  `AGENTS.md` (known gaps) or the PR.
- Do not create handoff, session, plan, checkpoint, report, or evidence files in
  the repository, and no new top-level or `docs/` markdown outside the table.
- Update an existing entry when a fact changes; delete entries that become false.
  State facts and rules, not history ("verified on X at revision Y" belongs in the PR).
- Write down only lasting, non-obvious knowledge: invariants, traps, regressions
  that were measured, gaps that remain open. Do not restate code or git history.
- Raw artifacts (EXRs, logs, reports) stay outside Git.

## Code rules (enforced by `cargo xtask --precommit` and clippy)

1. **Structure by modules, not by banners.** Do not mark subsections of a file with
   separator or banner comments (`// ----`, `// ==== Section ====`, `// --- foo ---`).
   If a file has parts that need a heading, those parts are submodules.
2. **Small files.** A Rust file has at most 800 lines, tests included. Split by
   responsibility into a directory module (`foo.rs` plus `foo/part.rs`; integration
   tests as `tests/name/main.rs` plus submodules). Do not split just to hit the number:
   each module must have one clear job and a name that says it.
3. **Self-explanatory code, few comments.** Name things so the code reads without
   comments. Do not write comments that restate the code, narrate steps, or mark
   sections. Keep only comments that explain why (a constraint, trap or invariant that
   is not visible), `// SAFETY:` comments, and doc comments on public items.
4. **Everything is Rust, every task is xtask.** Build, test, fixture generation,
   validation, benchmarks, packaging and release go through `cargo xtask`. Do not add
   Python, shell or other scripts. The only allowed non-Rust code is what must run in
   another runtime: the `circle-of-confusion` Python bindings and their tests,
   `test/natron/` (executed inside Natron's Python), the installer inputs in
   `packaging/`, and `.devcontainer/` setup. New exceptions need maintainer approval.
5. **Items are documented.** Every function, method, type, constant and module,
   public or private and including those inside submodules, has a doc comment that
   says what it does, what its arguments mean when that is not obvious, what it
   returns, and its `# Errors` and `# Panics` sections where they apply. Describe
   behaviour and constraints, not the signature. Enforced by the `missing_docs` and
   `clippy::missing_docs_in_private_items` lints (denied in CI).
6. **Every Rust file starts with a `//!` description** of what the file is and what
   it is responsible for, so a reader knows what they are looking at. One to three
   sentences; no history or change notes.
7. **A function does one job and fits on a screen.** Split by responsibility into named
   helpers or submodules; `clippy::too_many_lines` (150), `cognitive_complexity` (25),
   `excessive_nesting` (6) and `too_many_arguments` (10) back this up. Bundle related
   arguments in a struct instead of raising a limit.
8. **No repeated magic literals.** A number with a meaning gets a named, documented
   constant, and every use refers to it (`FOCUS_REPAIR_HALO`, not `7`).
9. **No duplicated blocks.** Repeated region maths, copy loops, bind-group boilerplate and
   similar code becomes one helper that the call sites share.
10. **Errors are typed.** Library errors are `thiserror` variants that name the cause, not
    strings or catch-alls. `unwrap` and `expect` are for tests only (`clippy::unwrap_used`,
    `expect_used`); library code returns the error or restructures so the case cannot occur.
11. **Discarded `Result`s are handled or explained.** `let _ = result` is rejected by
    `let_underscore_must_use`; propagate the error, handle it, or discard it with `.ok()`
    next to a comment that says why the failure is harmless. Library crates do not print
    (`print_stdout`, `print_stderr`).

## Working safely with other agents

- Check the branch, HEAD, worktree list, and working tree before editing. Fetch the
  requested branch when possible; do not reset to an old checkpoint blindly.
- Preserve user-owned changes and dirty primary checkouts. Use an isolated
  worktree for PR work; never copy unrelated primary changes into the PR.
- Agree on file ownership when agents work concurrently. Serialize GPU renders,
  benchmarks, and heavy builds; on shared POSIX hosts use the agreed advisory lock
  `/tmp/opendefocus-validation.lock`. A lock file's presence is not ownership.
- Do the smallest complete change. Reuse existing helpers, avoid new dependencies
  for small tasks, and remove dead code only after searching every caller.
- Keep generated binaries, EXRs, galleries, and reports out of Git. QA scripts
  require output outside the checkout; `xtask bench` uses gitignored
  `benchmark-results/`. Never commit personal agent settings or credentials.
- Do not keep handoff logs in Git. Put durable rules here, renderer invariants in
  `ARCHITECTURE.md`, and report results and gaps in the PR. Never claim older tests
  cover a changed shader or runtime path.

## Structure and architectural decisions

Rust 2024 workspace; stable Rust 1.95+ and native build tools. No Protoc, C++
compiler, or CMake is required.

- `crates/opendefocus`: public API, settings validation, WGPU orchestration,
  authored shaders in `src/kernel/shaders/`. The library is f32 RGBA only. Cargo
  features: `default = ["wgpu"]`, `wgpu`, and `serde`.
- Hand-written settings types live in `crates/opendefocus/src/datamodel/`; the
  GPU settings struct is `src/convolve_settings.rs`.
- `crates/opendefocus-ofx`: OpenFX plugin (`cdylib`) wrapping the renderer. The
  OFX API is hand-declared in `src/ofx_sys.rs`; there is no C++ or bindgen. The
  optional Nuke camera input (`NukeOfxCameraSuite`) is untested in real Nuke.
- `opendefocus-ui`: viewer; `opendefocus-cli`: standalone rendering and
  measurements.
- `xtask/`: cross-platform build, test, benchmark, and bundle coordination.
  `test/images/` contains fixtures.
- Removed packages `opendefocus-kernel`, `opendefocus-shared`,
  `opendefocus-datastructure`, and `opendefocus-nuke` must not be recreated.
- Native WGPU rendering does not use nightly Rust, generated SPIR-V, or an interpreter.
  The old `spirv-cli-build` crate is gone and must not be recreated.

## Renderer rules that must hold

- All product rendering goes through Adaptive WGPU, including previews. WGPU
  selects adapters, including its fallback adapter; there is no custom CPU or
  Legacy render fallback. Return initialization/resource/render errors.
- Rust host preparation (settings, CoC, aperture images, bounds, packed integral
  trees, uploads) is legitimate. There is no CPU convolution or pyramid renderer,
  product or test oracle; do not add one.
- Precision comes before the approximately one-second 1080p target for Accurate, which uses exact source-pixel integration. Fast is an explicit speed mode: it uses sample budgets and may use interpolation, denoising or upscaling, accepts small differences from Accurate, and must still keep sharp focused surfaces, HDR and premultiplied alpha; it must not hide artifacts with blur or truncation. Do not relax gates or change depth conventions to hide artifacts.
- Canonical prepared CoC is positive near (in front of the focus plane, `CocPolarity::PositiveForeground`) /
  negative far. Direct-CoC fixtures may declare another polarity. Nuke's bundled depth fixture requires 1/Z, including
  focal zero and negative depth; its zero-depth column is x=1379.
- ROI and stripes retain full-frame coordinates and pristine padded source.
  Astigmatic support can reach remote source chunks; do not crop that support.
  Read the GPU interface before changing traversal, padding, or streaming.
- The OFX plugin has not yet run in a real host. Natron and DaVinci Resolve checks
  are pending, and Nuke Non-Commercial does not load OFX plugins. Record host gaps
  as gaps; do not install or launch a host solely to satisfy them.

## Commands and verification

Use `cargo xtask --help` and crate READMEs for options.

```sh
cargo xtask --compile --cli
cargo xtask --compile --ui
cargo xtask --compile --gpu
cargo xtask --bundle-ofx
cargo xtask --package-ofx
cargo fmt --all -- --check
cargo xtask --clippy
cargo check --workspace --all-targets --locked
cargo test --release -p opendefocus --lib --locked -- --test-threads=1
cargo xtask --test-crates
uvx pre-commit run --all-files
```

Run relevant checks, not every expensive command for every task. Image tests need
release mode. Hardware tests are normally ignored: ordinary green unit tests do
not establish GPU parity. Run ignored hardware tests with `-- --ignored --test-threads=1`.
Naga checks shader syntax/layout, not driver behavior or rendered quality.
When adding a shader, update its embedded kernel constant, the xtask validator,
the native shader regression, and any matching buffer-layout assertions.

For rendering or allocation changes, capture unchanged-base and final release
measurements with identical hardware, features, inputs/settings, dimensions,
regions, and stripe height. Verify the recorded adapter. Restore pristine inputs;
measure cold initialization/first-frame separately, then three warm-ups and ten
measured frames. Use `cargo xtask bench before -- <input> [options]` and
`cargo xtask bench after`, or CLI `--repeat 13 --report <path>`. Report medians,
maxima, percentage changes, output correctness, and memory where relevant.
Investigate repeatable median regressions of 5% or more. Missing hardware is a
missing gate, never evidence of no impact.

Inspect matching base/final float outputs and contact sheets; use independent
PSF, energy, alpha, clipping, focus, and motion checks. Historical renders and the
committed gallery are comparisons, not truth labels. Numerical pass counts,
file hashes, and timings do not replace visual inspection or motion playback.
Documentation-only changes may skip render benchmarks; state that reason.

Rust: clippy is warning-free and CI denies warnings; fix a lint rather than
allowing it, and give any `allow` a reason. Surrounding style, four spaces, `snake_case` / `UpperCamelCase`, `cargo fmt`.
Python (only the allowed exceptions): Ruff formatting/lint, Google docstrings, `ty`
checks. Add focused
regressions near affected code and reuse fixtures. Prefer cross-platform `xtask`
automation. Use Conventional Commits, target PRs at `main`, and report validation
and limits without claiming production readiness from partial coverage.

## OFX plugin testing and known gaps

- Build with `cargo xtask --bundle-ofx`. Test headless with
  `cargo xtask --test-natron` (renders the occlusion scene through `NatronRenderer`
  and compares with the CLI at 1e-6 on the same machine). Scripts: `test/natron/`.
  Load a bundle without installing via `OFX_PLUGIN_PATH=<dir containing the bundle>`.
  Natron Python: node id `org.opendefocus.OpenDefocus`, input 0 Source, input 1
  Depth; Write `bitDepth` index 7 is 32f.
- `cargo xtask parity` renders `test/parity/commands.tsv` with the CLI into `target/parity/exr`
  and checks the plugin against it on the same machine. Nothing is stored outside the repo.
- Baseline EXRs are GPU-specific. On other hardware compare plugin against CLI on
  the same machine.
- Verified only on Apple M1 / Metal with Natron 2.6 and Resolve Free. Unverified:
  Linux/Windows, installers and signing, CI workflows (`test-natron` and the release
  workflows have never run), Resolve Studio, Nuke Studio/Indie, animated parameters, save/load,
  abort, 1x1 and odd sizes, non-zero RoD origins, pixel aspect, 8K, multi-plane layers.
- Releases are Linux only (OFX installers x86_64 and aarch64, Python wheels). macOS and Windows
  installers and wheels return once those platforms are tested in CI.
- Source premultiplication is assumed, not requested from the host.
- Astigmatism tiles request the full RoD and have only unit/mock coverage; clamping
  requests to host bounds changes the frame edge when the host gives less.
- Resolve renders 1080p Accurate in 15-22 s against 2 s in the CLI (same adapter);
  cause not found. Docs advise Fast while editing.
- 4K with astigmatism uses about 2 GB. There is no abort/timeout policy.
- Open question: Axial Aberration Blue/Yellow gave exact 0.0 in R in a few renders.
- Stale, non-reproduced report: ~90 GB memory growth in the Natron GUI.
- Missing tests: lens-centre regression (non-uniform render vs independent centre;
  tile/ROI keeps the full-frame centre). Baseline HDR/variable-radius manual
  aperture residual (3.0088725 vs 3.007865 at one pixel) is unresolved.
- Focal-zero Accurate renders take about 4.5 s at 1080p; any speed-up must keep exact
  integration and be measured, since shader control-flow changes have regressed.
