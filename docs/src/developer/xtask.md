# xtask

The xtask covers all development related tasks. It's really nice as it allows you to define developer actions which work cross-platform. Just have Rust installed, and everything works out of the box.

## Things it covers
- compilation `cargo xtask --compile --nuke-versions 15.0 --target-platform linux --use-zig`
- testing: `cargo xtask --test-crates` or `cargo xtask --pytest`
- precommit checks `cargo xtask --precomit`
- [ci tasks](https://codeberg.org/opendefocus/opendefocus/src/branch/main/.github/workflows/release_nuke.yaml)
    - [releasing](https://codeberg.org/opendefocus/opendefocus/src/branch/main/.github/workflows/release_nuke.yaml#L132)
    - [docs publishing](https://codeberg.org/opendefocus/opendefocus/src/branch/main/.woodpecker/release_docs.yaml#L14)
    - [again, compilation](https://codeberg.org/opendefocus/opendefocus/src/branch/main/.github/workflows/release_nuke.yaml#L53)
- docs previewing: `cargo xtask --serve-docs`