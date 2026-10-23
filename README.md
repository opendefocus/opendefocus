<h6 align="center">
  <br>
  <picture>
    <source media="(max-width: 768px)" srcset="https://raw.githubusercontent.com/opendefocus/opendefocus/main/resources/header_mobile.png">
    <img src="https://raw.githubusercontent.com/opendefocus/opendefocus/main/resources/header.png" alt="OpenDefocus" style="width: 100%;">
  </picture>
  <br></br>
  <p>Logo thanks to <a href="https://www.instagram.com/welmaakt/">Welmoed Boersma</a>!</p>
</h6>


<h4 align="center">An advanced open-source convolution library for image post-processing</h4>

<p align="center">
    <a href="https://crates.io/crates/opendefocus">
        <img src="https://img.shields.io/crates/l/opendefocus" alt="License" />
    </a>
    <a href="https://crates.io/crates/opendefocus">
        <img src="https://img.shields.io/crates/v/opendefocus" alt="Version" />
    </a>
    <a href = "https://github.com/opendefocus/opendefocus/issues">
      <img alt="GitHub Issues" src="https://img.shields.io/github/issues/opendefocus/opendefocus">
    </a>
      <a href="https://github.com/opendefocus/opendefocus/actions/workflows/test.yaml">
        <img src="https://github.com/opendefocus/opendefocus/actions/workflows/test.yaml/badge.svg" alt="Tests" />
    </a>
</p>


---

<p align="center">
  <a href="#features">Features</a> •
  <a href="https://opendefocus.github.io/opendefocus/download.html" target="_blank">Download</a> •
  <a href="https://opendefocus.github.io/opendefocus">Documentation</a> •
  <a href="https://github.com/opendefocus/opendefocus/issues" target="_blank">Issues</a> •
  <a href="./CHANGELOG.md" target="_blank">Changelog</a>
</p>

---

## Features

### User
* Entirely free! ([but please consider donating!](https://github.com/opendefocus/opendefocus))
* Optional camera input to match convolution to real world camera data.
* OpenFX plugin for OFX hosts, with tiled rendering.
* GPU accelerated (Vulkan/Metal)
* Both simple 2D defocus as well as depth based (1/Z, real or direct math based)
* Fast and Accurate quality modes: Fast for quick previews, Accurate for exact final renders.
* Lots of non uniform artifacts:
  * [Catseye](https://opendefocus.github.io/opendefocus/detailed/non_uniform/catseye.html)
  * [Barndoors](https://opendefocus.github.io/opendefocus/detailed/non_uniform/barndoors.html)
  * [Astigmatism](https://opendefocus.github.io/opendefocus/detailed/non_uniform/astigmatism.html)
  * [Axial aberration](https://opendefocus.github.io/opendefocus/detailed/non_uniform/axial_aberration.html)
  * [Inversed bokehs in foreground](https://opendefocus.github.io/opendefocus/detailed/non_uniform/inverse_foreground.html)
* Easy to use bokeh creator or use your own image


### Technical
* Process each pixel coordinate and channel on the image with a custom filter kernel. For a simple `RGBA` 1920x1080 image, that is at least 8.294.400 custom kernels!
* Rust host orchestration with native WGSL compute shaders, built using stable Rust.
* One Adaptive GPU renderer with exact Accurate integration and sample budgets for Fast.
* Library works on `f32` RGBA images only. Cargo features: `wgpu` (default), `serde` (settings serialization).
* Easy to use and open API to hook into your own application or DCC.
* Lots of control over the output, [take a look at all options available](https://docs.rs/opendefocus/latest/opendefocus/datamodel/index.html).

## Examples
### Simple convolution

```no_run
use anyhow::Result;
use image::{DynamicImage, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
use opendefocus::OpenDefocusRenderer;
use opendefocus::datamodel::Settings;

#[tokio::main]
async fn main() -> Result<()> {
    let mut settings = Settings::default();
    // set the defocus size in pixel radius, you can change whatever you want
    settings.defocus.circle_of_confusion.size = 25.0;

    // initialize a new renderer, this contains the runner instance (wgpu for example)
    // its fine to throw away if you only use one image, else its good to reuse to prevent initializing all the time
    let renderer = OpenDefocusRenderer::new(&mut settings).await?;

    // load an example image
    let image = image::load_from_memory(include_bytes!("../../../examples/simple/toad.png"))?.to_rgba32f();
    let mut array = image.to_ndarray();

    // then here we actually render
    renderer
        .render(settings, array.view_mut(), None, None)
        .await?;

    // just some loading to the image crate once again after rendering and storing it
    let image: Rgba32FImage = Rgba32FImage::from_ndarray(array)?;
    let image = DynamicImage::from(image).to_rgba8();
    image.save("./result.png")?;
    Ok(())
}

```

## Structure
The project has multiple crates defined at the crates directory:

| Crate Name                                                       | Description                                                                                                                   |
| ---------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| [opendefocus](./crates/opendefocus)                              | The actual public library itself. The `main` crate.                                                                           |
| [opendefocus-ofx](./crates/opendefocus-ofx)                      | OpenFX plugin that wraps the renderer. Not published to crates.io.                                                            |
| [opendefocus-cli](./crates/opendefocus-cli)                      | Standalone command-line renderer and measurement tool. Not published.                                                          |
| [opendefocus-ui](./crates/opendefocus-ui)                        | Native and WebAssembly evaluation viewer. Not published.                                                                       |
| [circle-of-confusion](crates/circle-of-confusion) | Camera and depth based circle of confusion calculator; independently versioned |

These supporting crates have a scope beyond convolution. These supporting crates are maintained in this workspace and versioned independently:
| Crate Name                                                                 | Description                                                                                                                                             |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [bokeh-creator](crates/bokeh-creator)             | Filter kernel generation library                                                                                                                        |
| [image-ndarray](crates/image-ndarray) | ImageBuffer and ndarray conversions; independently versioned |

The Nuke C++ plugin, Protobuf schemas, and the separate `opendefocus-shared` and
`opendefocus-datastructure` crates were removed; their code now lives in `opendefocus`.




## Build
For a normal library, CLI or viewer build, install stable [Rust](https://rustup.rs/)
1.95+ with your platform's native linker/build tools. See
[CONTRIBUTING.md](./CONTRIBUTING.md#tools-required) for the optional tools. Cargo downloads the Rust dependencies automatically. The renderer uses
authored WGSL and does not require a nightly shader compiler; the pinned nightly
is only for the historical SPIR-V utility.

```sh
cargo xtask --compile --cli
# Or build the viewer:
cargo xtask --compile --ui
```

GPU rendering uses authored WGSL embedded in `opendefocus`; no generated SPIR-V
shader is needed. Validate shader changes with `cargo xtask --compile --gpu` on
stable Rust. The pinned nightly toolchain is only for the historical SPIR-V
utility. Python tools and documentation tools are needed only for their
respective tasks.

Call `cargo xtask --help` for the available build and maintenance commands.

### Standalone evaluation application

Build and launch the native image/depth viewer with `cargo xtask ui`. To only build it, use
`cargo xtask --compile --ui` and run `target/release/opendefocus-ui` (`opendefocus-ui.exe` on
Windows). See the
[application README](./crates/opendefocus-ui/README.md) for controls, prerequisites,
and performance measurements.

The same viewer runs locally in a browser through WebAssembly and WebGPU. Install
the `wasm32-unknown-unknown` Rust target and `wasm-bindgen-cli 0.2.129`, then run
`cargo xtask --compile --web`. The `target/web/` bundle contains static hosting
assets and a self-contained `opendefocus-evaluation.html`; serve it over HTTPS or
localhost in a WebGPU-enabled browser. See the [browser instructions](./crates/opendefocus-ui/README.md#browser--webassembly).

### Command-line evaluation

Build with `cargo xtask --compile --cli`. The [OpenDefocus CLI](crates/opendefocus-cli/README.md)
uses the WGPU renderer for depth/custom bokeh inputs, full or striped rendering,
HDR output, JSON settings and separate initialization/render/end-to-end timings.

### OpenFX plugin

Build the plugin bundle with `cargo xtask --bundle-ofx`; it produces
`OpenDefocus.ofx.bundle` for the host platform. Copy the bundle to the OFX plugin
directory of your system:

- macOS: `/Library/OFX/Plugins`
- Windows: `%CommonProgramFiles%\OFX\Plugins`
- Linux: `/usr/OFX/Plugins`

`cargo xtask --package-ofx` builds a platform installer (macOS `.pkg`, Windows Inno Setup `.exe`, Linux `.tar.gz` with `install.sh`).

### Working with coding agents

[AGENTS.md](AGENTS.md) is the shared starting point for coding
agents. [ARCHITECTURE.md](ARCHITECTURE.md) is the renderer technical reference.

