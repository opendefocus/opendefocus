<h6 align="center">
  <br>
  <picture>
    <source media="(max-width: 768px)" srcset="./resources/header_mobile.png">
    <img src="./resources/header.png" alt="OpenDefocus" style="width: 100%;">
  </picture>
  <br></br>
  <p>Logo thanks to <a href="https://www.instagram.com/welmaakt/">Welmoed Boersma</a>!</p>
</h6>


<h4 align="center">An advanced open-source convolution library for image post-processing</h4>

<p align="center">
    <a href="https://ci.codeberg.org/repos/15492">
        <img src="https://ci.codeberg.org/api/badges/15492/status.svg" alt="Tests" />
    </a>
    <a href="https://crates.io/crates/opendefocus">
        <img src="https://img.shields.io/crates/l/opendefocus" alt="License" />
    </a>
    <a href="https://crates.io/crates/opendefocus">
        <img src="https://img.shields.io/crates/v/opendefocus" alt="Version" />
    </a>
    <a href="https://img.shields.io/badge/nuke-15%2B-yellow?logo=nuke">
        <img src="https://img.shields.io/badge/nuke-15%2B-yellow?logo=nuke" alt="Nuke Versions" />
    </a>
</p>

---

<p align="center">
  <a href="#features">Features</a> •
  <a href="https://codeberg.org/olivebowl/circle-of-confusion/releases" target="_blank">Download</a> •
  <a href="https://opendefocus.codeberg.page">Documentation</a> •
  <a href="./CHANGELOG.md" target="_blank">Changelog</a> •
</p>

---

## Features

### User
* Entirely free!
* Native integration for camera data to match convolution to real world camera data.
* GPU accelerated (Vulkan/Metal)
* Both simple 2D defocus as well as depth based (1/Z, real or direct math based)
* Custom quality option, for quick renders with less precision or heavier higher precision renders.
* Lots of non uniform artifacts:
  * [Catseye](https://opendefocus.codeberg.page/detailed/non_uniform/catseye.html)
  * [Barndoors](https://opendefocus.codeberg.page/detailed/non_uniform/barndoors.html)
  * [Astigmatism](https://opendefocus.codeberg.page/detailed/non_uniform/astigmatism.html)
  * [Axial aberration](https://opendefocus.codeberg.page/detailed/non_uniform/axial_aberration.html)
  * [Inversed bokehs in foreground](https://opendefocus.codeberg.page/detailed/non_uniform/inverse_foreground.html)
* Easy to use bokeh creator or use your own image
* Foundry Nuke native plugin (through [CXX](https://cxx.rs) FFI). Basically a wrapper around the Rust crate ([serves](./crates/opendefocus-nuke/) as a good developer reference on how to integrate it in other DCC's or applications!).


### Technical
* Process each pixel coordinate and channel on the image with a custom filter kernel. For a simple `RGBA` 1920x1080 image, that is at least 8.294.400 custom kernels!
* 100% written in pure Rust (stable channel) without external library dependencies.
* Same algorithm on GPU and CPU with same source code (thanks to Rust-GPU spirv compiler).
* Easy to use and open API to hook into your own application or DCC.
* Lots of control over the output, [take a look at all options available](https://docs.rs/cc/latest/opendefocus/datamodel/struct.Settings.html).

## Structure
The project has multiple crates defined at the crates directory:

| Crate Name                                                       | Description                                                                                                                   |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| [opendefocus](./crates/opendefocus)                              | The actual public library itself. The `main` crate.                                                                           |
| [opendefocus-datastructure](./crates/opendefocus-datastructure/) | Datastructure bindings to protobuf and implementations.                                                                       |
| [opendefocus-kernel](./crates/opendefocus-kernel/)               | Kernel (`no-std`) source code. Runs on both GPU and CPU.                                                                      |
| [opendefocus-macros](./crates/opendefocus-macros/)               | Just some centralized macro definitions crate-wide.                                                                           |
| [opendefocus-nuke](./crates/opendefocus-nuke/)                   | Nuke specific source code. Includes both C++ and Rust.                                                                        |
| [opendefocus-shared](./crates/opendefocus-shared/)               | Code that can be used by both the [kernel](./crates/opendefocus-kernel/) and main [opendefocus](./crates/opendefocus/) crate. |
| [spirv-cli-build](./crates/spirv-cli-build/)                     | Wrapper around the SPIR-V from Rust-GPU to compile using nightly for `opendefocus-kernel`.                                    |


Besides that, these crates have been located outside of this repository as they have a bigger scope than just convolution:
| Crate Name                                                                 | Description                                                                                                                                             |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [circle-of-confusion](https://codeberg.org/olivebowl/circle-of-confusion) | Circle of confusion algorithm to calculate the actual circle of confusion from real world camera data or create a nice depth falloff based on selection |
| [bokeh-creator](https://codeberg.org/olivebowl/bokeh-creator)             | Filter kernel generation library                                                                                                                        |
| [inpaint](https://codeberg.org/olivebowl/inpaint)                         | Telea inpainting algorithm in pure Rust                                                                                                                 |



## Build
You need to have Rust installed:
Both the stable version (1.92+) as well as the nightly version listed in [spirv-cli-build/rust-toolchain.toml](./crates/spirv-cli-build/rust-toolchain.toml)

That's it basically.

All compilation is handled through xtasks. Call `cargo xtask --help` for more information.

### Nuke
For Nuke building you need additional dependencies. As the linking process needs to have the DDImage library and headers installed, the xtask fetches these sources automatically.

For extracting of the archives, `msiextract` needs to be installed for Windows installs.

In theory C++11 is supported so it would work with Nuke 10 and higher, but that is not tested.

When compiling Nuke and fetching sources, to speed up downloading it is recommended to either use a machine in the USA or use a VPN. Because its an AWS bucket in the US-East region which does not have geo-replication enabled.

Example to compile Nuke 15:
```bash
cargo xtask \
  --compile \
  --gpu \
  --nuke-versions 15.1,15.2 \
  --target-platforms linux \
  --output-to-package
```

This will create the package with Linux compiled binaries in the [package](./package/).

Now you are able to copy this folder to some other location, or add it to your `NUKE_PATH` and it should show up.

```bash
export NUKE_PATH=$(pwd)/package

# launch your nuke etc...
```
