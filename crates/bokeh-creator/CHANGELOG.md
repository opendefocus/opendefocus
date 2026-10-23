# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/opendefocus/opendefocus/compare/bokeh-creator-v0.1.17...bokeh-creator-v0.2.0) - 2026-10-23

### Added

- *(bokeh-creator)* add the bokeh-creator crate

### Fixed

- *(bokeh-creator)* import ToString in the no_std datamodel test

### Build

- Remove unused dependencies and align test dependencies with the workspace.

### Changed

- Upgrade the public array interface to ndarray 0.17.2 and image-ndarray 0.1.6.

## [0.1.17](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.16...v0.1.17) - 2026-01-09

### Added

- Add `protobuf-vendored` feature

## [0.1.16](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.15...v0.1.16) - 2026-01-09

### Other

- Add protobuf compiler by default

## [0.1.15](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.14...v0.1.15) - 2025-12-16

### Other

- Merge branch 'main' of ssh://codeberg.org/olivebowl/bokeh-creator
- Fix uneven resolution to output to correct position in image

## [0.1.14](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.13...v0.1.14) - 2025-12-06

### Other

- Add documented crate

## [0.1.13](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.12...v0.1.13) - 2025-11-25

### Other

- Set prost to use no default features by default and optionally enable in std

## [0.1.12](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.11...v0.1.12) - 2025-11-25

### Other

- release v0.1.11
- Add renderer feature to optionally compile only datastructure

## [0.1.11](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.10...v0.1.11) - 2025-11-25

### Other

- Add compile test and add render to array behind image feature gate

## [0.1.10](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.9...v0.1.10) - 2025-11-25

### Other

- Fix ndarray feature gate to match image-ndarray

## [0.1.9](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.8...v0.1.9) - 2025-11-25

### Other

- Set image-ndarray prelude behind feature get

## [0.1.8](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.7...v0.1.8) - 2025-11-25

### Other

- Make image-ndarray optional

## [0.1.7](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.6...v0.1.7) - 2025-11-14

### Other

- Revert object naming to original naming

## [0.1.6](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.5...v0.1.6) - 2025-11-14

### Other

- Fix dependencies and add libm feature for backend selection of glam
- Rename settings and renderer to BokehSettings and BokehRenderer for unique naming

## [0.1.5](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.4...v0.1.5) - 2025-11-11

### Other

- Switch to protobuf and optionally use serde for structs

## [0.1.4](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.3...v0.1.4) - 2025-11-11

### Other

- Set libnoise to default features false to support wasm targets

## [0.1.3](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.2...v0.1.3) - 2025-10-28

### Other

- Add support for no-std without noise and add ndarray support

## [0.1.2](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.1...v0.1.2) - 2025-10-20

### Other

- Fix code in readme

## [0.1.1](https://codeberg.org/olivebowl/bokeh-creator/compare/v0.1.0...v0.1.1) - 2025-10-20

### Other

- Include docs and fix images in readme to show up on crates.io
