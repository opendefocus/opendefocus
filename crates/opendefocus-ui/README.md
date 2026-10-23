# OpenDefocus evaluation application

A standalone native and WebAssembly viewer for evaluating the library without an OFX host. The control
groups follow the Nuke documentation: **Defocus**, **Bokeh**, and **Non-uniform**.

Build and launch through xtask:

```sh
cargo xtask ui
```

To build without launching, run `cargo xtask --compile --ui` and start
`./target/release/opendefocus-ui` yourself.

On Windows, launch `target\release\opendefocus-ui.exe`. Cargo's configured target
directory applies. Rust and native build tools are the only prerequisites, just as
for the library. The UI build targets the host platform; cross-compilation is not covered here.
The authored WGSL shaders are embedded in the binary; adding `--gpu` validates
them with Naga before building. This uses stable Rust.

Use the **Test images** menu to load a bundled sample without browsing for files.
These existing fixtures were made by the project author and are included with
their permission for testing and evaluation. They are embedded in the executable,
so samples also work when the application is moved outside the repository.

| Sample | Resolution | Useful for |
| --- | --- | --- |
| Toad | 512 × 512 | Texture, detail, and quick 2D comparisons |
| Color bars | 256 × 256 | Color edges, blending, and aberration |
| Occlusion + depth | 1920 × 1080 | Foreground/background boundaries and performance |

Selecting a sample resets render controls and clears the custom filter. The
occlusion sample loads its matching EXR automatically and sets the maximum
radius to 10 px. Depth mode defaults to **1/Z**; keep this setting for the
Nuke-rendered occlusion sample. Other samples use 2D defocus.

1. Load an image (PNG, JPEG, TIFF, EXR, or HDR).
2. Optionally load a depth image of exactly the same dimensions. The red channel
   is used unchanged, including floating-point and signed EXR values. Select
   Real, 1/Z, or Direct math to match the depth source. Loading a new source
   image clears depth. A failed load preserves the previous inputs.
   Loading a depth file or bundled sample resets math to 1/Z.
3. Adjust sliders or type numeric values. Auto render waits until you release
   the mouse; disable it to make several changes and render manually.
4. Scroll over the viewer to zoom around the cursor. Drag with the middle mouse
   button to pan. **Fit** centers the whole image; **1:1** resets to pixel size.
   Use Show original to compare the input.
5. In depth mode, a small focus dot stays visible in the image. For Real or 1/Z
   depth, drag it or click the image to set the focal plane from that pixel's raw
   depth value. This works in fit and scrolled
   zoomed and panned views. Auto render updates the result; with it disabled,
   press Render.
   As in Nuke, the dot is disabled for Direct maps, whose values already
   specify blur radii. Turn off Preview filter before picking. Zero depth is
   accepted unchanged; negative or nonfinite picked values display an error.

Defocus supports focal-plane visualization, protect, maximum radius, pixel
aspect, size multiplier, and manual physical camera settings. Bokeh supports
disc/bladed shapes, filter preview, resolution, ring/blur controls and noise.
The Bokeh tab follows Nuke's Simple, Disc, Bladed, and Image filter types and
control order. All controls stay visible. Select Disc or Bladed to enable shape
and noise controls; blades, angle, and curvature require Bladed. Aspect ratio
also works with Simple. Image uses the loaded filter and disables shape controls.
Preview filter is available in every filter mode and uses the library's bokeh
preview path. Disc and Bladed previews use the same resolution/aspect format as
Nuke and can be rendered without loading a source or depth image.
A cropped custom filter image can also be loaded. Non-uniform controls expose
catseye, astigmatism, barndoors, axial aberration, their inversions, and the
screen-space center. The quality choice is Fast (sampled gather: fastest, approximate) or Accurate (exact source-pixel integration: slowest, highest precision) and applies to all groups; the viewer starts on Accurate. The viewer renders through WGPU.

Rendering updates the viewer progressively as each horizontal stripe completes.
During rendering, a thin white line marks the bottom of the latest completed
stripe; it disappears when the render completes.
Stripe height uses the same automatic library policy as Nuke; there is no extra
control. Each stripe samples the original full-resolution float image with the
padding required by the renderer. Processing is limited to the visible area;
zooming and panning automatically refresh newly exposed regions after navigation
settles, including when Auto render is off. Fit view renders the full image.
An entirely offscreen image does not render. Filter preview uses one full-image
pass to preserve its shape.

A single background worker keeps the viewer responsive, and the renderer/device
is reused across renders. The fixed-height status bar shows
progress, the actual GPU device, elapsed milliseconds, and
megapixels/second. Timing sums the library's `render_stripe` calls, including
preprocessing and GPU transfers; loading, input copies, device initialization and
display conversion are excluded. Progressive rendering can take longer overall
while providing earlier updates. Filter/focal-plane previews measure their preview
path, so switch them off when comparing convolution performance.

The viewer displays values clamped to 8-bit RGBA; it does not provide Nuke's
color management or an HDR display transform. Input values passed to the
library are preserved. Nuke node graphs, animation, and camera-file import
are outside this evaluation app.

## Browser / WebAssembly

The browser build runs this same viewer and PR11's Adaptive renderer locally,
using WebAssembly for Rust and WebGPU for convolution. Input files are decoded
in Rust, including float EXR/HDR and the unchanged red depth channel. Files are
not uploaded. The native executable remains available.

Install Rust's browser target and the JavaScript glue generator (its version
must match `wasm-bindgen` in `Cargo.lock`):

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
cargo xtask --compile --web
python3 -m http.server 8080 --directory target/web
```

Open `http://localhost:8080`. Serve the output directory via HTTP(S); double
clicking the multi-file `index.html` as a `file://` URL cannot load the WebAssembly
module. The self-contained evaluation HTML avoids those asset requests, but
WebGPU availability on `file://` still depends on the browser; localhost is the
reliable evaluation route.
Cargo's target-directory configuration applies to both the build and `web/`.
The output contains `index.html`, `opendefocus.js`, `opendefocus_bg.wasm`, and
binding declarations. `opendefocus-evaluation.html` embeds the JavaScript and
WebAssembly into one file for convenient sharing and evaluation. Copy the entire
directory when hosting. All asset URLs are relative, so both `/` and repository/subdirectory GitHub Pages URLs work.
No Node, Trunk, CDN, server API, shared memory, or special isolation headers
are required at runtime.

A WebGPU-enabled browser on HTTPS (or localhost) is required, with the
`float32-filterable` GPU feature used by the renderer. The opening page explains
missing WebGPU support; device/resource errors appear in the viewer's status
bar. There is no alternate CPU render path. Browser adapter limits may reject
large frames that work on native Metal/Vulkan; start with the small bundled
samples. Decoding and host preparation run on the browser event loop and can
pause the UI on large inputs; GPU work and file pickers are awaited without
blocking on a native thread. The existing 8-bit display limitation also applies.

The **WebAssembly viewer** workflow builds and uploads `opendefocus-web` without
deploying it. When integrated on main, the existing **Documentation** workflow
includes the viewer at `/demo/` in the same Pages artifact, preserving the docs
site. The first build requires the new files from this branch; do not deploy an
artifact from main before merging the PR11 prerequisite and this change.

Focused check:

```sh
cargo test --release -p opendefocus-ui
```
