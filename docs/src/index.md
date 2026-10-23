<h6 align="center">
  <br>
  <picture>
    <source media="(max-width: 768px)" srcset="https://raw.githubusercontent.com/opendefocus/opendefocus/main/resources/header_mobile.png">
    <img src="https://raw.githubusercontent.com/opendefocus/opendefocus/main/resources/header.png" alt="OpenDefocus" style="width: 100%;">
  </picture>
  <br>
  <p>Logo thanks to <a href="https://www.instagram.com/welmaakt/">Welmoed Boersma</a>!</p>
</h6>


<h4 align="center">An advanced open-source convolution library for image post-processing</h4>

---

<p align="center">
  <a href="./donate.md">Donate</a> •
  <a href="#user-features">Features</a> •
  <a href="./user_guide.md">User guide</a> •
  <a href="./demo/">Browser viewer</a> •
  <a href="./download.md">Download</a> •
  <a href="./installation.md">Installation</a> •
  <a href="https://github.com/opendefocus/opendefocus/issues" target="_blank">Issues</a>
</p>

---

## New to OpenDefocus?
The best way to start is by reading the [<i class="fa-solid fa-rocket"></i> Quickstart](quickstart/index.md). This user targeted guide contains everything you need to know to get the most out of OpenDefocus.

Looking for specific features? Look at the [detailed](./detailed.md) section which describes all the features more thouroughly.

---

## <i class="fa-solid fa-star"></i> User features

* Entirely free! ([but please consider donating!](./donate.md))
* Native integration for camera data to match convolution to real world camera data.
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


## <i class="fa-solid fa-gears"></i> Technical features
* Process each pixel coordinate and channel on the image with a custom filter kernel. For a simple `RGBA` 1920x1080 image, that is at least 8.294.400 custom kernels!
* Written in Rust (stable channel) with WGSL shaders run through WGPU; no Protobuf, C++ or CMake toolchain is needed.
* One GPU renderer for both quality modes, distributed as an OpenFX plugin, a CLI and a viewer.
* Easy to use and open API to hook into your own application or DCC.
* Lots of control over the output, [take a look at all options available](https://docs.rs/opendefocus/latest/opendefocus/datamodel/index.html).


