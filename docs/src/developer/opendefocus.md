# opendefocus

This crate is the public rendering API. `OpenDefocusRenderer` initializes WGPU
and renders scenes through one Adaptive GPU pipeline. Accurate integrates exact
source-pixel weights; Fast uses its sample budget.
Initialization and rendering failures return errors. WGPU selects the adapter,
including its fallback adapter; there is no separate CPU or Legacy product path.

## Source map

Paths below are relative to `crates/opendefocus/src/`:

| Path | Responsibility |
| --- | --- |
| `lib.rs` | Public renderer, supported-setting validation, full/stripe APIs, and data-model exports |
| `error.rs`, `abort.rs` | Render errors and shared cancellation |
| `runners/wgpu.rs`, `runners/adaptive_gpu.rs` | WGPU resources and shader dispatch |
| `runners/adaptive.rs` | GPU buffer layouts, layers, pyramids, and diagnostics |
| `kernel/shaders/` | Authored and embedded WGSL programs |
| `kernel/stages/` | Shared host preparation and test-only algorithm references |
| `worker/adaptive_render.rs` | Full-frame, ROI, stripe, and streamed rendering plans |
| `renders/` | Aperture/image resizing and focal-plane preview helpers |

The former `opendefocus-kernel`, `opendefocus-shared` and `opendefocus-datastructure`
crates are now modules inside this crate. There is no CPU renderer; the Rust code
only prepares host data and dispatches WGPU work.

## Coordinates and image data

Rendering uses linear, premultiplied RGBA32F. Canonical prepared CoC is positive
near and negative far. Direct-CoC input supplies signed pixel radii; real depth,
1/Z, and physical camera settings convert through the public data model. Nuke's
bundled fixture uses 1/Z, including focal zero and negative depth.

ROI and stripe calls retain full-frame coordinates and receive pristine padded
source context. Use `renderer.padding(settings)` and keep the original color
and depth immutable when assembling stripes. Astigmatic support can reach remote
source chunks; limiting context to the destination region changes the result.

## Validation

`cargo xtask --compile --gpu` checks all five WGSL sources with Naga on stable
Rust. Shader syntax and ordinary unit tests do not establish hardware rendering
quality. Renderer changes also need explicit ignored GPU tests, independent
image/motion checks, and matched release benchmarks.

[ARCHITECTURE.md](https://github.com/opendefocus/opendefocus/blob/main/ARCHITECTURE.md)
records buffer and coordinate invariants.

## Settings datamodel

The shared OpenDefocus settings are plain Rust structs and enums in [`src/datamodel/`](https://github.com/opendefocus/opendefocus/tree/main/crates/opendefocus/src/datamodel). `mod.rs` holds conversions and helper methods; `model.rs` holds the types. Messages nested in a message are modules (for example `defocus::Settings` or `render::Filter`). Enum fields are stored as `i32` with typed getters and setters. Serde derives sit behind the `serde` feature. This used to be the separate `opendefocus-datastructure` crate.
