# Contributing to OpenDefocus

Contributing to OpenDefocus is greatly appreciated and welcome! To start, I'd recommend to look at the open issues. Or create one yourself and start on that if you have some suggestions for improvements.

Once done, please create a PR to the `main` branch and we can discuss (and hopefully) merge it.

## Development

### Working with coding agents

Read [AGENTS.md](AGENTS.md) and [ARCHITECTURE.md](ARCHITECTURE.md).
Agents should start at `AGENTS.md`. Keep project knowledge in these shared documents, record open gaps in
`AGENTS.md`, and keep personal settings out of Git.

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
need the correctness and performance checks in [AGENTS.md](AGENTS.md).

The imported Python crates in `crates/` have their own environments, test
commands, and CI.

### Getting Started

1. Fork the repository to your **GitHub** account.

   ```bash
   git clone git@github.com:YOUR_USERNAME/opendefocus.git
   cd opendefocus/
   ```
2. Create a branch with a descriptive name.
   - It is recommended to give your branch a meaningful name, relevant to the feature or fix you are working on.
     - Good examples:
       - `docs-plugin-installation`
       - `feature-new-nonuniform-artifact`
       - `fix-panic-during-this-operation`

3. Commit using the [conventional commits specifications](https://www.conventionalcommits.org/en/v1.0.0/)

4. Run the development environment. Everything is handled through the xtask.

5. Create your contribution and test the changes.

---

## Most of all
Thank you!! For taking the time to read this and to contribute your time into this project.
