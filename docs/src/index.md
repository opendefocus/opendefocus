<h6 align="center">
  <br>
  <picture>
    <source media="(max-width: 768px)" srcset="https://codeberg.org/opendefocus/opendefocus/media/branch/main/resources/header_mobile.png">
    <img src="https://codeberg.org/opendefocus/opendefocus/media/branch/main/resources/header.png" alt="OpenDefocus" style="width: 100%;">
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
  <a href="./download.md">Download</a> •
  <a href="./installation.md">Installation</a> •
  <a href="https://codeberg.org/opendefocus/opendefocus/issues" target="_blank">Issues</a>
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
* Custom quality option, for quick renders with less precision or heavier higher precision renders.
* Lots of non uniform artifacts:
  * [Catseye](https://opendefocus.codeberg.page/detailed/non_uniform/catseye.html)
  * [Barndoors](https://opendefocus.codeberg.page/detailed/non_uniform/barndoors.html)
  * [Astigmatism](https://opendefocus.codeberg.page/detailed/non_uniform/astigmatism.html)
  * [Axial aberration](https://opendefocus.codeberg.page/detailed/non_uniform/axial_aberration.html)
  * [Inversed bokehs in foreground](https://opendefocus.codeberg.page/detailed/non_uniform/inverse_foreground.html)
* Easy to use bokeh creator or use your own image
* Foundry Nuke native plugin (through [CXX](https://cxx.rs) FFI). Basically a wrapper around the Rust crate ([serves](./crates/opendefocus-nuke/) as a good developer reference on how to integrate it in other DCC's or applications!).


## <i class="fa-solid fa-gears"></i> Technical features
* Process each pixel coordinate and channel on the image with a custom filter kernel. For a simple `RGBA` 1920x1080 image, that is at least 8.294.400 custom kernels!
* 100% written in pure Rust (stable channel) without external library dependencies.
* Same algorithm on GPU and CPU with same source code (thanks to Rust-GPU spirv compiler).
* Easy to use and open API to hook into your own application or DCC.
* Lots of control over the output, [take a look at all options available](https://docs.rs/opendefocus/latest/opendefocus/datamodel/index.html).


