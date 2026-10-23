# OpenDefocus CLI

An unpublished workspace binary for evaluating the OpenDefocus renderer without
the viewer. Scene renders always use the public adaptive renderer; there is no
legacy algorithm switch or legacy fallback. Library versions and release
packaging remain unchanged.

```sh
cargo xtask --compile --cli
target/release/opendefocus-cli test/images/any/toad.png \
  --output result.exr --radius 12 --quality accurate

target/release/opendefocus-cli color.exr --depth depth.exr \
  --settings lens.json --output result.exr \
  --render-mode striped --stripe-height 64 --repeat 3 --report timings.json
```

Rendering always uses WGPU and requires the `gpu`/`wgpu` feature. A
`--no-default-features` build can dump and validate settings, but scene renders
return an error because the public renderer requires WGPU. Runtime render errors
are returned as errors; the renderer does not switch algorithms.

Inputs support PNG, JPEG, TIFF, EXR and Radiance HDR. Color and custom bokeh are
loaded as f32 RGBA; depth uses the red channel verbatim and must match the color
size. `--depth` supplies depth for depth mode; `--bokeh` supplies an image filter.
2D mode, image and generated apertures, non-square pixels, non-uniform lens
effects, and striped rendering use the same public renderer.
`--quality fast` selects sampled gathers: the fastest option, approximate.
`--quality accurate` selects exact source-pixel integration, with bounds built for
the selected aperture mip hierarchy: the slowest option, highest precision. The
supported-setting quality and performance gates are still being validated; this
README makes no production parity or speed claim.
Crop custom bokeh to its bounds. No color-space transform, exposure, tone
mapping or premultiplication is added: pixel values follow the renderer/UI's
decoding.

`--classifier-overlay FILE` optionally runs a GPU tile-classification diagnostic
and writes its overlay as PNG. It is independent of rendering and does not
change the output. The timing report includes `classifier_ms` and a tile summary
only when this diagnostic is requested. It requires a GPU-enabled build.

Outputs follow their extension:

| Format | Storage |
| --- | --- |
| `.exr`, `.tif`, `.tiff` | f32 RGBA, retains HDR and alpha |
| `.hdr` | Radiance RGBE, retains HDR with format precision; discards alpha |
| `.png` | 16-bit RGBA, clips to 0–1 |
| `.jpg`, `.jpeg` | 8-bit RGB, clips to 0–1; discards alpha, lossy |

Images and JSON reports are written to temporary files beside their destinations
and replace existing files only after writing succeeds. Failed encoding preserves
the previous output; saving through a hard-link alias cannot change the source.

Full rendering is the default. Striped rendering uses the renderer's recommended
stripe height unless overridden, samples each padded band from the immutable
source/depth, retains global coordinates, and copies only the unpadded band into
the output. Very small bands may add substantial setup overhead. UI and OFX tile
requests likewise render padded source context in full-frame coordinates and
return only the requested region. Full, striped, and cropped comparisons must
use matching output regions; quality gates for their parity remain under review.

## Settings

`--settings FILE` accepts a partial JSON overlay on `Settings::default()` with the
same field names and numeric enum values as the renderer data model. Unknown
fields are errors. `--dump-settings` prints the complete effective settings
without loading images or initializing a device, so it can generate an editable
starting point:

```sh
target/release/opendefocus-cli --dump-settings > settings.json
```

CLI radius, quality and input mode flags override the file. `--radius`
sets the CoC size used by 2D and settings-derived depth CoC; with
`use_direct_math`, depth values are already CoC values and pass through
unchanged. This is a background render (`render.gui = false`), so
`render.farm_quality` applies; its default is accurate. `--quality fast|accurate`
sets both quality fields. Resolution is always taken from the color input. `--bokeh` selects Image
filter mode and therefore requires its image even if a stale `filter.preview`
flag is set. Generated bokeh preview applies only when both `filter.preview` is
true and mode is `BokehCreator`.

Simple inverse-foreground phase uses the configured filter-box resolution. The
saved Legacy Simple 8×8 placeholder is not a substitute for that configured
resolution; the phase preserves the historical one-filter-texel offset of
`2 * radius / filter_resolution` per axis.

Example `lens.json` (real depth, focal plane in input depth units):

```json
{
  "defocus": {
    "circle_of_confusion": {
      "math": 0,
      "focal_plane": 5.0,
      "max_size": 20.0,
      "camera_data": {
        "focal_length": 50.0,
        "f_stop": 2.8,
        "world_unit": 3,
        "filmback": { "width": 36.0, "height": 24.0 }
      }
    },
    "camera_max_size": 20.0
  },
  "render": { "center": { "x": 960.0, "y": 540.0 } },
  "non_uniform": { "catseye": { "enable": true, "amount": 0.4 } }
}
```

A camera object is filled from `CameraData::default()` before applying its partial
overlay; set it to `null` to disable it. World units: 0 mm, 1 cm, 2 dm, 3 m, 4 in,
5 ft. Depth math: 0 real, 1 inverse (1/Z). For signed input radii, use
`{"defocus":{"use_direct_math":true}}` and omit camera data. Generated bokeh
uses `render.filter.mode = 1` (simple = 0, image = 3); set `bokeh.filter_type = 1`
for blades (disc = 0). All lens/noise settings are exposed in the dumped model,
including the noise seed for repeatability.

## Timing and validation

Successful renders print JSON to stdout and optionally save the same JSON with
`--report`. It includes effective settings, input paths, dimensions, actual
backend/device, stripe height, and these wall-clock measurements:

- `load_ms`: input decoding and validation.
- `initialization_ms`: renderer/device initialization, once per process.
- `frames[].renderer_calls_ms`: awaited renderer calls, including renderer setup,
  preprocessing and readback, excluding CLI copies and stripe assembly.
- `frames[].frame_ms`: fresh source/depth/filter copies, renderer calls and stripe
  assembly. Each `--repeat` starts from pristine source and reuses the renderer;
  the first frame is recorded rather than discarded as a warmup.
- `classifier_ms`: optional tile diagnostic time, separate from rendering.
- `save_ms`: conversion and image encoding for the final frame only.
- `end_to_end_ms`: settings, loading, initialization, all frames and image saving;
  excludes report serialization/writing and process/runtime startup.

Record the Git revision, command and input hashes alongside reports when comparing
renderer changes. Keep backend/device, render mode and stripe height fixed.
EXR/TIFF outputs make pixel comparisons possible without an
8-bit preview conversion. Timing is wall time, not individual GPU kernel time.
Ctrl-C sets the renderer's shared cancellation flag; a cancelled render fails
without saving its partially rendered frame. Renderer errors remain errors.

```sh
cargo test --release -p opendefocus-cli
cargo test --release -p opendefocus-cli --no-default-features
# Hardware check (serialize GPU workloads and run outside restricted sandboxes):
cargo test --release -p opendefocus-cli test_cli_and_library_use_the_same_wgpu_renderer -- --ignored
```

The CLI joins the existing `cargo xtask --test-crates` CI workspace test run.
