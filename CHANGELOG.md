# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.2](https://github.com/opendefocus/opendefocus/compare/v0.2.1...v0.2.2) - 2026-10-25

### Fixed

- *(ci)* use the Natron Windows archive and make a test type unambiguous on Windows

## [0.2.1](https://github.com/opendefocus/opendefocus/compare/v0.2.0...v0.2.1) - 2026-10-24

### Fixed

- *(render)* make the far occlusion and the front hand-over continuous
- *(render)* blend a nearly fully hidden far gather to the far surface colour
- *(render)* hide the far background behind less blurred far surfaces
- *(render)* remove the sharp line and ticks at the focal plane

### Other

- *(render)* tidy far occlusion and Fast composite code

## [0.2.0](https://github.com/opendefocus/opendefocus/compare/v0.1.10...v0.2.0) - 2026-10-23

### Added

- add Nuke version to 12 and higher

### Fixed

- build opendefocus without the wgpu feature
- *(shader)* avoid pointers to struct members so the SPIR-V writer accepts Accurate
- sample points are now determined circular to prevent unnecessary calculations and fix the simple shape to be circular
- Change donate url

### Other

- rewrite the user guide for the OpenFX plugin
- [**breaking**] replace the Nuke plugin crates with the WGPU renderer
- update readme
- update packages to 0.1.10
- release v0.1.9
- Merge pull request '#24: Fix simple shape to be circular when using depth' ([#57](https://github.com/opendefocus/opendefocus/pull/57)) from 24-fix-simple-shape-to-be-circular-when-using-depth into main
- array resizing to always render in 4 channels internally
- release v0.1.8
- add optional protoc build to compile protobuf only on linux builds
- release v0.1.7
- add donate button in plugin and in readme
- add issues badge and change order
- release v0.1.6
- Set release version from 13+
- release v0.1.5
- Add downloads table and changelog page to documentation, add examples and switch interface to expect a ArrayViewMut instead of a reference
- Add metadata and switch input arrayviewmut object to be the object itself instead of a mut ref of arrayviewmut
- Update to 0.1.4
- Remove dev dependency in opendefocus crate and set futures to minor version
- release v0.1.3
- Revert link buttons
- Update changelog link
- Update to 0.1.2
- Remove build dependencies opendefocus crate
- Update crates/opendefocus/CHANGELOG.md
- release v0.1.1
- Set glam to 0.30
- Move shaders and remove doc include
- Set hard version in cargo.toml for crate deps
- Update changelog link
- Remove deny.toml
- Merge pull request 'Set crate package parameters' ([#18](https://github.com/opendefocus/opendefocus/pull/18)) from develop into main
- Set crate package parameters
- Merge pull request 'Set kernel, shared, datastructure as publishable crates' ([#16](https://github.com/opendefocus/opendefocus/pull/16)) from develop into main
- Set kernel, shared, datastructure as publishable crates
- Update crates/opendefocus/CHANGELOG.md
- release v0.1.0
- Fix argument in cargo xtask in readme
- Fix username and headers
- Update badge
- Add christmas header and add readme
- Fix release docs and switch to codeberg releases
- Simplify compilation
- Add install of uv for test and add pytest as dev dependency
- Add shader wgsl source in subfolder for crate publication
- Initial commit

### Changed

- **Breaking:** the Nuke plugin is replaced by an OpenFX plugin (`opendefocus-ofx`). Rendering now goes through a native WGPU renderer (Vulkan/Metal) instead of the previous kernel crates; there is no CPU or legacy fallback.
- Quality has two modes: `Fast` (renders at half resolution with sampled gathers and exact discs for bright highlights with a plain aperture, about 3x faster than the old Low and 15x or more faster than Accurate on 1080p frames, within roughly 50 dB PSNR of Accurate on the test scenes) and `Accurate` (exact, default). `Low`, `Medium`, `High`, `Custom` and the Samples parameter are removed.
- The user guide is rewritten for the OpenFX plugin.

### Added

- OpenFX plugin with tiled rendering and an optional camera input.
- `opendefocus-cli`, a command line renderer, and `opendefocus-ui`, a viewer.
- Linux and Windows OFX installers.
- `circle-of-confusion`, `bokeh-creator` and `image-ndarray` crates inside the workspace.
- `cargo xtask` automation for building, testing, benchmarking and packaging, plus CI for tests, Natron host rendering, PR render previews and releases.
- Contributing guide and architecture notes.

### Removed

- The `opendefocus-nuke`, `opendefocus-kernel`, `opendefocus-shared`, `opendefocus-datastructure` and `spirv-cli-build` crates.

### Fixed

- Building `opendefocus` without the `wgpu` feature.
- Accurate shaders failing SPIR-V validation because of pointers to struct members.

## [0.1.10](https://codeberg.org/opendefocus/opendefocus/releases/tag/v0.1.10) - 2026-01-29

### Fixed

- #62: fix focal point causing the Nuke node to be corrupted when direct math is selected


## [0.1.9](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.8...v0.1.9) - 2026-01-23

### Added
- #2 Up to 4 channels renders are now supported.

### Fixed

- #24 simple shape being circular when in background in depth mode. Now prevents samples determined by circular distance, this results in a small performance boost as well.
- #23 preview filter is enabled again and the crash is fixed

## [0.1.8](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.7...v0.1.8) - 2026-01-10

### Fixed

- Fix #47 which did not find the right symbols on Linux ([pr #53](https://codeberg.org/opendefocus/opendefocus/pulls/53))

## [0.1.7](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.6...v0.1.7) - 2026-01-08

### Added

- Documentation button in plugin
- Donate button in plugin and readme

### Fixed

- #47 'wrong version' and #45 'The specified procedure could note be found' because of a caching during the build step of the plugin. Plugin should now load correctly.
- filter preview is now temporarily disabled, because it is blocked by #23

## [0.1.6](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.5...v0.1.6) - 2025-12-31

### Added

- Nuke 13

## [0.1.5](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.4...v0.1.5) - 2025-12-30

### Added

- Add Nuke 14 and add [downloads](https://opendefocus.codeberg.page/download.html) page to documentation

## [0.1.4](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.3...v0.1.4) - 2025-12-25

### Fixed

- Fix Nuke release CI build

## [0.1.3](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.2...v0.1.3) - 2025-12-25

### Added

- Add license.md to release package

## [0.1.2](https://codeberg.org/opendefocus/opendefocus/compare/v0.1.1...v0.1.2) - 2025-12-25

### Fixed

- Simplify build process

## [0.1.1](https://codeberg.org/opendefocus/opendefocus/releases/tag/v0.1.1) - 2025-12-25

### Added

- Add Nuke 15 and 16

## [0.1.0](https://codeberg.org/opendefocus/opendefocus/releases/tag/opendefocus-datastructure-v0.1.0) - 2025-12-24

### Added

- Initial release
