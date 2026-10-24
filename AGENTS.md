# OpenDefocus agent guide

OpenDefocus is a GPU (WGPU) defocus renderer for VFX: a Rust library with a CLI,
a viewer, an OpenFX plugin and Python bindings. This file is the entry point for
coding agents. Instructions from the maintainer take precedence over it.

## Read first

| Before you | Read |
| --- | --- |
| start any task | this file, then the GitHub issue you are working on |
| touch the renderer, shaders, GPU resources or allocation | [ARCHITECTURE.md](ARCHITECTURE.md) |
| validate, benchmark or test the OFX plugin | [TESTING.md](TESTING.md) |
| handle unsafe code, host input, CI or dependencies | [SECURITY.md](SECURITY.md) |
| write an issue, PR, commit or fixture (all public) | [SECURITY.md](SECURITY.md#personal-information) |
| use or change a crate | that crate's `README.md` |

Read the source before trusting a note or a historical result.

## Workflow: issue, branch, pull request

Work is driven by GitHub issues on `opendefocus/opendefocus`.

1. **Start from a ready issue.** If the task has no issue, create one from the
   [Task template](.github/ISSUE_TEMPLATE/task.yml) (`gh issue create` with the
   same headings in the body). An issue is ready when someone who missed the
   discussion can implement and verify it without asking: it has a goal,
   context, in- and out-of-scope lists, checkable acceptance criteria and the
   verification to run. If an issue is not ready, comment with the open
   questions instead of guessing.
2. **Branch from current `main` in a worktree**, named `<type>/<issue>-<slug>`,
   for example `fix/142-roi-padding`.
3. **Make the smallest complete change** that meets the acceptance criteria.
   File anything out of scope as a new issue rather than fixing it on the side.
4. **Open a pull request against `main`** using the
   [pull request template](.github/pull_request_template.md). Link the issue with
   `Closes #<n>`, show how each acceptance criterion was verified, and list what
   was not verified.
5. **Record what remains as issues.** Gaps, follow-ups and open questions found
   during the work become issues, not notes in the repository.

Commits follow [Conventional Commits](https://www.conventionalcommits.org/).
Labels: `bug`, `enhancement`, `documentation`; add `good first issue` or
`help wanted` where it fits.

## Where knowledge goes

Write down only what a future reader cannot recover from the code, in one place:

| Kind of knowledge | Home |
| --- | --- |
| Rules, commands and workflow for working in the repo | `AGENTS.md` |
| Why the renderer is built this way: invariants, layouts, failure modes, measured lessons | `ARCHITECTURE.md` |
| Verification gates, benchmark protocol, host testing, coverage gaps | `TESTING.md` |
| Vulnerability reporting and security rules | `SECURITY.md` |
| Crate usage and options | that crate's `README.md` |
| End-user and developer guides | `docs/src/` (mdbook) |
| Planned work, bugs, open questions | GitHub issues |
| Results of a task: validation, measurements, limits | the pull request |

- `README.md` is for users: no test status, host gaps or internals.
- No handoff, session, plan, report or evidence files. `cargo xtask --precommit`
  rejects Markdown outside the allowed locations.
- State facts and rules, not history. Edit an entry when a fact changes and
  delete it when it becomes false.
- Raw artifacts (EXRs, logs, reports, galleries) stay outside Git.

## Code rules

Enforced by `cargo xtask --precommit` and clippy; CI denies warnings.

1. **Modules, not banners.** No separator comments (`// ----`, `// === X ===`).
   A part that needs a heading is a submodule.
2. **Files stay under 800 lines**, tests included. Split by responsibility
   (`foo.rs` plus `foo/part.rs`; integration tests as `tests/name/main.rs` plus
   submodules), and give each module one job and a name that says it.
3. **Few comments.** Comments explain why (constraint, trap, invariant), plus
   `// SAFETY:` and doc comments. Never restate code or narrate steps.
4. **Everything is Rust, every task is `cargo xtask`.** No Python, shell or other
   scripts, except code that must run in another runtime: the
   `circle-of-confusion` Python bindings and tests, `test/natron/`, and
   `packaging/`. New exceptions need maintainer approval.
5. **Every item has a doc comment**, private ones included: what it does, the
   meaning of non-obvious arguments, the return value, and `# Errors` /
   `# Panics` where they apply.
6. **Every Rust file starts with `//!`**: one to three sentences on what the file
   is responsible for.
7. **A function does one job and fits on a screen.** Limits: 150 lines, cognitive
   complexity 25, nesting 6, 10 arguments. Bundle arguments in a struct instead
   of raising a limit.
8. **Named constants** for meaningful numbers (`FOCUS_REPAIR_HALO`, not `7`).
9. **No duplicated blocks.** Repeated region maths, copy loops and bind-group
   setup become one shared helper.
10. **Typed errors.** Library errors are `thiserror` variants that name the cause.
    `unwrap`/`expect` only in tests.
11. **No silently discarded `Result`s.** Propagate, handle, or use `.ok()` with a
    comment saying why the failure is harmless. Library crates do not print.

Fix lints rather than allowing them; an `allow` needs a `reason`. Use `cargo fmt`
and the surrounding style. Python (allowed exceptions only): Ruff, Google
docstrings, `ty`.

## Renderer rules

Details and reasons are in [ARCHITECTURE.md](ARCHITECTURE.md).

- All product rendering, previews included, goes through Adaptive WGPU. WGPU
  picks the adapter, including its fallback. There is no CPU convolution or
  pyramid renderer, as product or as test oracle; do not add one. Return
  initialization, resource and render errors.
- Accurate means exact source-pixel integration; precision comes before the
  ~1 s 1080p target. Fast may sample, interpolate or upscale, but must keep sharp
  focused surfaces, HDR and premultiplied alpha, and must not hide artifacts
  with blur or truncation.
- Never relax a gate or change a depth convention to hide an artifact.
  Canonical CoC is positive near, negative far; the Nuke depth fixture is 1/Z.
- ROI and stripes keep full-frame coordinates and pristine padded source. Do not
  crop astigmatic support. Read the GPU interface before changing traversal,
  padding or streaming.
- A new shader also needs its embedded kernel constant, the xtask validator, the
  native shader regression and any buffer-layout assertions.
- Do not recreate removed packages: `opendefocus-kernel`, `opendefocus-shared`,
  `opendefocus-datastructure`, `opendefocus-nuke`, `spirv-cli-build`. No nightly
  Rust, generated SPIR-V or interpreter.

## Repository map

Rust 2024 workspace, stable Rust 1.95+. No Protoc, C++ compiler or CMake.

| Path | Contents |
| --- | --- |
| `crates/opendefocus` | Public API, settings (`src/datamodel/`, GPU struct in `src/convolve_settings.rs`), WGPU orchestration, WGSL in `src/kernel/shaders/`. f32 RGBA only. Features: `wgpu` (default), `serde`. |
| `crates/opendefocus-ofx` | OpenFX plugin (`cdylib`). OFX API hand-declared in `src/ofx_sys.rs`; no bindgen. |
| `crates/opendefocus-cli`, `crates/opendefocus-ui` | Standalone renderer and measurements; viewer. |
| `crates/circle-of-confusion`, `bokeh-creator`, `image-ndarray` | Independently versioned support libraries. |
| `xtask/` | Build, test, benchmark, bundle and release automation. |
| `test/` | Image fixtures, parity commands, Natron scripts. |

## Commands

`cargo xtask --help` lists everything.

```sh
cargo xtask --compile --cli          # or --ui, --gpu (Naga shader validation)
cargo xtask --bundle-ofx             # --package-ofx for installers
cargo fmt --all -- --check
cargo xtask --clippy
cargo check --workspace --all-targets --locked
cargo test --release -p opendefocus --lib --locked -- --test-threads=1
cargo xtask --test-crates
uvx pre-commit run --all-files
```

Run the checks relevant to the change. Image tests need `--release`. Rendering,
shader and allocation changes also need the hardware gates in
[TESTING.md](TESTING.md); green unit tests do not establish GPU parity.
Documentation-only changes may skip render benchmarks; say so in the PR.

## Working alongside other agents

- Check branch, HEAD, worktrees and working tree before editing. Fetch the branch
  you need; never reset to an old checkpoint blindly.
- Preserve user changes and dirty checkouts. Do PR work in its own worktree and
  never carry unrelated changes into it.
- Agree on file ownership when working concurrently. Serialize GPU renders,
  benchmarks and heavy builds using the advisory lock
  `/tmp/opendefocus-validation.lock`; a lock file's presence is not ownership.
- Reuse existing helpers, avoid new dependencies for small tasks, and remove dead
  code only after searching every caller.
- Never commit generated binaries, EXRs, reports, personal agent settings,
  credentials or personal information (see
  [SECURITY.md](SECURITY.md#personal-information)).
