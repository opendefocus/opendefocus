# Contributing to OpenDefocus

Contributing to OpenDefocus is greatly appreciated and welcome!

## How work is organised

Work is driven by [GitHub issues](https://github.com/opendefocus/opendefocus/issues).

1. Pick an open issue, or open one with the **Task** or **Bug report** template.
   A task describes the goal, context, scope, acceptance criteria and how to
   verify it, so anyone can pick it up. Comment on the issue before you start
   so work is not duplicated.
2. Work on a branch named `<type>/<issue>-<slug>`, for example
   `fix/142-roi-padding`.
3. Open a pull request against `main` that says `Closes #<issue>` and shows how
   each acceptance criterion was verified.

Report security vulnerabilities privately, as described in [SECURITY.md](SECURITY.md).

## Development

### Working with coding agents

Agents start at [AGENTS.md](AGENTS.md), which links to
[ARCHITECTURE.md](ARCHITECTURE.md), [TESTING.md](TESTING.md) and
[SECURITY.md](SECURITY.md). Keep personal agent settings out of Git.

### Tools Required

Install only the tools needed for your task:

| Task | Additional tools beyond stable Rust 1.95+ and native linker/build tools |
| --- | --- |
| Library, CLI or viewer | Nothing beyond stable Rust 1.95+ and native linker/build tools |
| Validate authored GPU shaders | `cargo xtask --compile --gpu` uses Naga on stable Rust. Shader changes also need hardware correctness and performance checks. The excluded historical SPIR-V utility alone uses its pinned nightly. |
| Python tests and checks | [uv](https://docs.astral.sh/uv/getting-started/installation/); each Python crate under `crates/` manages its own environment. |
| Contributor pre-commit hooks | Run `uvx pre-commit run --all-files`; the hook runs `cargo xtask --precommit`, which uses `uvx` for `ty` and validates shaders when kernel files change. |
| Build or serve documentation | `cargo install --locked cargo-about mdbook mdbook-toc` |
| Browser viewer | `rustup target add wasm32-unknown-unknown`, then `cargo install --locked wasm-bindgen-cli --version 0.2.129`; build with `cargo xtask --compile --web`. |
| OpenFX plugin bundle | Nothing beyond stable Rust 1.95+ and native linker/build tools; build with `cargo xtask --bundle-ofx`. |

A normal CLI build is `cargo xtask --compile --cli`; a viewer build is
`cargo xtask --compile --ui`. Neither requires Python tooling or nightly Rust.
Avoid installing the documentation tools unless you work on documentation.

### Checks before a pull request

Run `cargo fmt --all -- --check` and `cargo xtask --test-crates` for Rust changes.
Use `cargo xtask --compile --gpu` for shader validation; hardware changes also
need the correctness and performance checks in [TESTING.md](TESTING.md).

The imported Python crates in `crates/` have their own environments, test
commands, and CI.

### Getting Started

1. Fork the repository to your **GitHub** account.

   ```bash
   git clone git@github.com:YOUR_USERNAME/opendefocus.git
   cd opendefocus/
   ```
2. Create a branch for the issue, for example `fix/142-roi-padding`.

3. Commit using the [conventional commits specifications](https://www.conventionalcommits.org/en/v1.0.0/)

4. Run the development environment. Everything is handled through the xtask.

5. Create your contribution and test the changes.

---

## Most of all
Thank you!! For taking the time to read this and to contribute your time into this project.
