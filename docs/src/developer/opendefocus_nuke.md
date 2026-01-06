# opendefocus-nuke

This crate is basically a binary wrapper. It is the application used in Nuke. The Nuke API is used by creating bindings using CXX.

## Structure
- [build.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/build.rs): build for plugin and bindings to Nuke NDK.
- [opendefocus.hpp](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/include/opendefocus.hpp): C++ specific headers
- [src](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src): all C++ and Rust source code
  - [lib.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src/lib.rs): C++ side code Plugin definition and source code.
  - [opendefocus.cpp](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src/channels.rs): replication of Nuke NDK Channels and bindings.
  - [channels.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src/channels.rs): replication of Nuke NDK Channels and bindings.
  - [consts.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src/consts.rs): user knob definitions
  - [knobs.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src/knobs.rs): wrapper for the C++ code invocation which creates the knob in Nuke land
  - [render.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/src/render.rs): Render handler to call the Rust library with the provided data