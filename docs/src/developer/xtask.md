# xtask

Xtask coordinates development tasks across platforms. See [building](./building.md)
for the tools required by each task, and run `cargo xtask --help` for all options.

## Things it covers
- standalone evaluation UI: `cargo xtask --compile --ui` (builds the host executable in `target/release`)
- browser viewer: `cargo xtask --compile --web` (static assets and an embedded HTML evaluation file in `target/web`)
- standalone renderer CLI: `cargo xtask --compile --cli`
- shader validation: `cargo xtask --compile --gpu`
- testing: `cargo xtask --test-crates`
- precommit checks `cargo xtask --precommit`
- [docs publishing](https://github.com/opendefocus/opendefocus/blob/main/.github/workflows/release_docs.yaml)
- docs previewing: `cargo xtask --serve-docs`
