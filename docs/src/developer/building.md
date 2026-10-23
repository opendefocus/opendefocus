# Building

Build from the repository root with [xtask](./xtask.md). Normal builds require
stable Rust 1.95+ and native linker/build tools. Cargo downloads the Rust
dependencies.

```sh
cargo xtask --compile --cli
cargo xtask --compile --ui
cargo xtask --compile --gpu
```

The first two commands build the release CLI and native viewer. `cargo xtask --bundle-ofx`
builds the OpenFX bundle `OpenDefocus.ofx.bundle`. The last validates
the authored WGSL shaders with Naga. The renderer embeds WGSL directly; normal
builds do not use nightly Rust or generated SPIR-V.

Rendering requires a WGPU adapter; initialization errors are returned.

## Browser viewer

```sh
rustup target add wasm32-unknown-unknown
cargo install --locked wasm-bindgen-cli --version 0.2.129
cargo xtask --compile --web
python3 -m http.server --directory target/web 8080
```

Open `http://localhost:8080/` in a WebGPU-enabled browser. `target/web/` contains
the HTML/JS/WASM hosting bundle and `opendefocus-evaluation.html` with the module
embedded. HTTPS or localhost is required for the reliable browser preview.
The documentation deployment workflow places the viewer under `/demo/`.

## Checks

```sh
cargo fmt --all -- --check
cargo xtask --test-crates
```

Rust tests run the complete workspace in release mode with the locked dependency
versions.
Ignored hardware tests must be run explicitly for renderer changes
(`cargo test --release -p opendefocus --lib --locked -- --ignored --test-threads=1`).

For local documentation, install `cargo-about`, `mdbook`, and `mdbook-toc`, then
run `cargo xtask --serve-docs`. Use `cargo xtask --help` for the remaining tasks.
