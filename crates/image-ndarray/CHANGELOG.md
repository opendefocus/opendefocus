# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- Preserve allocation capacity and pixel order when converting owned sliced or non-standard-layout arrays into images.
- Accept extra ImageBuffer backing storage without constructing an ndarray with a mismatched length.

## [0.1.6](https://codeberg.org/olivebowl/image-ndarray/compare/v0.1.5...v0.1.6) - 2026-10-07

### Other

- install release-plz with cargo-binstall
- update dependencies and move releases to Woodpecker
- Update CHANGELOG.md
- release v0.1.5
- Restrict all dependencies to strict range
- Restrict ndarray
- Update CHANGELOG.md
- release v0.1.4
- Merge branch 'main' of ssh://codeberg.org/olivebowl/image-ndarray
- Add support for converting to 2d array
- Update CHANGELOG.md
- release v0.1.3
- Replace badge with woodpecker
- Fix ndarray dependency for no-std
- Fix ndarray dependency for no-std
- Merge branch 'main' of ssh://codeberg.org/olivebowl/image-ndarray
- Add woodpecker test, switch to thiserror and disable tests when image is not enabled
- Update CHANGELOG.md
- release v0.1.2
- Add support for only using NormalizedFloat trait
- Add float normalizer trait
- Update CHANGELOG.md
- release v0.1.1
- Rename docs and tests
- Merge branch 'main' of ssh://codeberg.org/olivebowl/image-ndarray
- Rename as_mut_ndarray to as_ndarray_mut
- Update CHANGELOG.md
- release v0.1.0
- Add additional keywords and categories
- Rename package in Cargo.toml and rename test
- Initial commit

## [0.1.5](https://codeberg.org/olivebowl/image-ndarray/compare/v0.1.4...v0.1.5) - 2025-11-12

### Other

- Restrict all dependencies to strict range

## [0.1.4](https://codeberg.org/olivebowl/image-ndarray/compare/v0.1.3...v0.1.4) - 2025-10-31

### Other

- Add support for converting to 2d array

## [0.1.3](https://codeberg.org/olivebowl/image-ndarray/compare/v0.1.2...v0.1.3) - 2025-10-31

### Other

- Switch to thiserror with conditional `std` feature and disable tests when `image` is not enabled

## [0.1.2](https://codeberg.org/olivebowl/image-ndarray/compare/v0.1.1...v0.1.2) - 2025-10-28

### Other

- Add NormalizedFloat trait to convert to and from floats to (unsigned) ints

## [0.1.1](https://codeberg.org/olivebowl/image-ndarray/compare/v0.1.0...v0.1.1) - 2025-10-27

### Other

- Rename as_mut_ndarray to as_ndarray_mut

## [0.1.0](https://codeberg.org/olivebowl/image-ndarray/releases/tag/v0.1.0) - 2025-10-27

### Other

- Initial release
