# Developer documentation
Thank you for considering to contribute to OpenDefocus! In this documentation you will find (hopefully) all you need to get started.

---

## Structure
The project has multiple crates defined at the crates directory, each of them serve their own purpose:

| Crate Name                                                       | Description                                                                                                                   |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| [opendefocus](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus)                              | The actual public library itself. The `main` crate.                                                                           |
| [opendefocus-datastructure](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-datastructure/) | Datastructure bindings to protobuf and implementations.                                                                       |
| [opendefocus-kernel](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-kernel/)               | Kernel (`no-std`) source code. Runs on both GPU and CPU.                                                                      |
| [opendefocus-nuke](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-nuke/)                   | Nuke specific source code. Includes both C++ and Rust.                                                                        |
| [opendefocus-shared](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-shared/)               | Code that can be used by both the [kernel](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-kernel/) and main [opendefocus](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus/) crate. |
| [spirv-cli-build](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/spirv-cli-build/)                     | Wrapper around the SPIR-V from Rust-GPU to compile using nightly for `opendefocus-kernel`.                                    |


### Located outside of opendefocus repository

Besides that, these crates have been located outside of this repository as they have a bigger scope than just convolution:
| Crate Name                                                                 | Description                                                                                                                                             |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [circle-of-confusion](https://codeberg.org/olivebowl/circle-of-confusion) | Circle of confusion algorithm to calculate the actual circle of confusion from real world camera data or create a nice depth falloff based on selection |
| [bokeh-creator](https://codeberg.org/olivebowl/bokeh-creator)             | Filter kernel generation library                                                                                                                        |
| [inpaint](https://codeberg.org/olivebowl/inpaint)                         | Telea inpainting algorithm in pure Rust                                                                                                                 |


