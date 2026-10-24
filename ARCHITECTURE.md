# Architecture: adaptive GPU renderer

Why the renderer is built the way it is: stages, invariants, buffer layouts,
failure modes and lessons from measured regressions. The rules that follow from
it are summarised in [AGENTS.md](AGENTS.md#renderer-rules); verification is in
[TESTING.md](TESTING.md).

All public scene rendering uses the Adaptive WGPU pipeline. Initialization and rendering errors are returned without switching to another algorithm. There is no Legacy algorithm selector, CPU renderer, or runtime algorithm fallback. WGPU owns adapter selection, including its fallback adapter. Tests compare GPU output with small analytic references (disc coverage, pyramid averages), never with a CPU renderer.

## Stages

`WgpuRunner::classify_tiles` accepts linear premultiplied RGBA32F, signed CoC in full-resolution pixel-radius units, the original-frame origin, and the maximum radius. Its 16×16 grid stays aligned to global coordinates, including cropped and odd-sized frames. Eleven fields describe each tile's CoC, alpha, luminance, contrast, highlights, reason bits, class, and adjacent CoC step. Reason bits are depth=1, alpha=2, highlight=4, texture=8; classes are sharp=0, smooth blur=1, detail=2. The overlay is diagnostic and does not alter rendering.

`prepare_adaptive_layers` takes an explicit `CocPolarity` and produces separate near/far premultiplied color, CoC, geometric validity, and foreground-edge distance. Background continuation uses bounded nearest-seed searches. It extends visible surfaces; a single flattened image cannot reveal hidden geometry.

`build_adaptive_pyramid` averages each layer separately. CoC averages only valid pixels within that layer. Odd trailing rows and columns enter the final texel of each floor-sized mip level.

Accurate integrates exact source-pixel weights through conservative integral trees. Fast uses a fixed sample budget and renders at half resolution (`worker/adaptive_render/half.rs`): the source and CoC are averaged on the global 2x2 grid, the shared pipeline runs with halved lens centre, resolution and size, and the result is bilinearly upsampled. Pixels that are in focus and that the half-size render barely changed keep their full-resolution source, so sharp surfaces stay sharp. With a plain disc aperture, small pixels far brighter than their neighbours are removed from the half-size gather and drawn as exact discs (`worker/adaptive_render/scatter.rs`), because a sampled gather hits them with few samples and shows spiral ribs; discs that cross the frame edge stay with the gather, and discs are hidden where a nearer in-focus or foreground surface covers them. Sampled tiers interpolate tile transitions. Far gathers use area-distributed positions; near gathers include a radial Jacobian. Custom RGBA apertures use their mip hierarchy. Spatial lens effects retain Legacy's coordinate, inversion, and axial-channel conventions.

## Native shaders

The authored WGSL sources live in `crates/opendefocus/src/kernel/shaders/` and are embedded through constants in the renderer's private kernel module. Preparation, classification, pyramid reduction, packing, convolution, and focal/bokeh preview rendering all use native WGSL. `cargo xtask --compile --gpu` validates all five sources with Naga on stable Rust. There is no runtime SPIR-V generation or interpreter. Rust constructs resources, CoC/aperture inputs, bounds and packed trees; all convolution runs on the GPU.

Convolution has sampled and exact entry points. Exact plain circles use a compile-time specialization and preorder subtree ends. When Accurate uses a plain circle, the radius exceeds two pixels, and at least half of valid near-layer entries equal the radius cap, a fused specialization walks the near tree once for front radiance and cap-potential occupancy. It uses the fused potential only when front presence equals the cap; other radii use the regular exact potential traversal. Spatial and custom apertures descend uncertain cells rather than discarding them. The explicit gather layout uses eight storage buffers. Gather uniforms occupy 64 bytes, lens uniforms 160 bytes, integral nodes 64 bytes, and aperture bounds 32 bytes. Aperture range queries read appended 32-byte mip-layout records from the existing bounds buffer; the public bounds payload and diagnostic APIs keep their original format. For custom image apertures without spatial or axial effects, a padded UV interval is first rejected only when its closest point lies provably outside the normalized clip disk. Remaining cells use one shared RGBA filter-range query for all channels; uncertain intervals still descend to exact leaves. Spatial and axial cases retain their separate channel calculations. Integral nodes are built directly in their packed GPU layout, avoiding a simultaneous raw-node copy.

## Settings and source context

Canonical CoC is positive near (in front of the focus plane) and negative far; `adaptive_coc_from_depth` negates the calculator output to produce it. Direct input passes through verbatim; depth and physical-camera conversion use `adaptive_coc_from_depth`. The Nuke fixture stays on 1/Z, including focal zero and negative depth. 2D mode uses a constant radius and requires no depth.

The same pipeline handles Simple, generated and Image apertures, lens controls, physical camera settings, focal-plane visualization, generated filter preview, and padded ROI/stripe rendering. Preview requires BokehCreator mode. Source dimensions and GPU binding limits are checked before allocation. GPU allocation limits use the enabled adapter's actual limits, subject to the renderer's per-buffer maximum; streamed Accurate planning caps each preparation or accumulator buffer at 512 MiB. This cap is not an estimate of total adapter memory. Preparation needs 52 bytes per source pixel.

ROI callers must request `renderer.padding(settings)` of pristine source context and retain full-frame coordinates. When an astigmatic Accurate request exceeds the ordinary block plan, exact Accurate streaming partitions the source into disjoint, globally aligned cores. Each core is prepared with a halo covering the jump-flood dependency chain, initialization neighborhood, and silhouette cleanup; halo pixels are context only and never contribute twice. Optical distances continue to use full-frame coordinates. Near radiance, validity, and maximum contributing radius are accumulated before potential occupancy, local coverage, and far terms; normalization and premultiplied composition happen once after all source cores contribute. One-pixel cores retain their implicit leaf root. Astigmatic Fast requests that exceed their existing adapter limits remain unsupported and return an error; other requests that cannot fit an available plan also return an error.

Convolution gathers only the nonempty absolute `process_region`; low-level outputs outside that region are zero, and an empty region requests the full frame. The ordinary block path retains its prepared source context. The streamed astigmatic Accurate path prepares source chunks, then releases normalized depth and redundant full-frame source/CoC copies before gathering. Its output accumulator stores 72 floats (288 bytes) per output pixel. A separate two-storage-buffer compaction entry point copies the four finalized RGBA floats into a 16-byte-per-pixel output buffer before readback; it performs no color arithmetic and does not enlarge the eight-storage-buffer gather layout. The compact staging buffer replaces the old 288-byte-per-pixel staging buffer. The gather covers the requested region plus the seven-pixel context needed for focus reconstruction. It retains selected immutable source textures, prepared base layers and packed tree buffers within a 1 GiB per-frame cache; resources outside that budget are uploaded and released per destination as needed. Host-side prepared and source chunks still scale with total frame area, with additional copies where chunk halos overlap, so this bounds cached GPU resources and individual working allocations rather than total host or process memory.

Focus reconstruction reads prepared pixels from the owning source cores across chunk seams. For ROI output, the stream gathers the seven-pixel reconstruction context and copies only the requested region back to the caller. HDR values, premultiplied fractional alpha, and Nuke 1/Z conversion retain their existing semantics.

## Non-obvious invariants and failure modes

- Generated filter preview requires both `filter.preview` and `BokehCreator`
  mode. A stale flag in Image mode still requires the custom aperture. Generated
  bokeh aspect zero is valid; Image mode ignores that aspect setting. Preview
  uniforms are 80 bytes; row chunks retain global coordinates and cap each
  output buffer at 64 MiB. Bokeh preview does not upload unused source/depth.
  Skip `atan2` at the exact lens center: Metal can return NaN for `(0, 0)`;
  the center angle is the configured angle, matching the aperture generator.
- GPU normalized UV samples put texel centers at `(n + 0.5) / resolution`.
  Any host-side reference sampler must use `uv * resolution - 0.5` for linear
  sampling.
- Simple inverse-foreground phase uses the configured filter-box resolution,
  with a one-filter-texel displacement of `2 * radius / filter_resolution` per
  axis. The saved Legacy 8x8 Simple placeholder is not that resolution.
- Conservative aperture intervals are proofs of aggregation eligibility, not
  discard predicates: uncertain nodes descend to leaves. Include the half-pixel
  circular coverage fringe (2.5-pixel support for a 2-pixel disk). Lens cells can
  aggregate only when spatial opacity and aperture weights are constant. Reuse
  equal-scale bounds queries only for equal signed axial scales.
- Axial aberration above amount 8 can reverse and enlarge RGB CoC beyond the
  base radius. Padding includes `max(1, abs(1 - abs(amount) / 4))`; preserve native
  per-channel signs and phase rather than enlarging a scalar radius alone.
- Disjoint source cores align to the global 16-pixel tile grid. Preparation
  halos include the sum of jump offsets, seed neighborhood and cleanup reach.
  Crop prepared output to the owning core: overlapping contexts never contribute
  twice. Unsigned shader geometry is full-frame-local; signed lens origins and
  negative global tile origins must still be preserved.
- Stack/texture/buffer limits must be checked before allocation. Thin panoramas
  can fit a pixel budget while exceeding a texture axis. Preorder subtree ends
  are strictly forward and skip only contained descendants. Eight storage
  bindings are required; automatic layout inference can remove unused bindings,
  so convolution uses its explicit shared resource contract.
- Focus reconstruction must distinguish an isolated subpixel crossing from a
  real focused surface. Keep opaque focused silhouettes sharp. Rebuild a
  crossing only when the `CoC` reaches 0.75 px within half a pixel on both
  sides; a resolvable ramp (receding floor, curved silhouette) keeps its sharp
  row, and rebuilding it draws streaks. A near centre keeps its own source under
  the local layer until it enters the front regime (`CoC` 1 to 2); a far centre
  hands over to the far gather only above the 0.5 px gather inclusion radius.
  Earlier, lower thresholds leaked the occluded far continuation into the first
  near row and drew a line along the focal plane. Fractional alpha
  is premultiplied coverage, not geometric validity. Restore opaque coverage only
  after proving the original source alpha plane is opaque (`alpha >= 1 - 1e-6`
  everywhere). A single opaque point on transparent black must blur its alpha.
  Streaming computes the proof once across every disjoint source core, including
  remote cores, and passes it through cached and uncached submissions. Test-only
  render references apply the same coverage policy before focus reconstruction.
  Keep independent transparent-point energy gates alongside those references.
- No setting, host, or saved state may select a CPU render path; there is none.
  Reproduce the bundled
  depth case with 1/Z, including negative depth and the x=1379 zero column.

## Lessons from measured regressions

An opaque-foreground early return that was algebraically exact slowed an earlier
native Metal case by about 9.7% and was reverted. Fewer operations are not a GPU
speed measurement: control flow and register pressure require matched hardware
runs. Native circular scalar/fused specializations preserve exact integration;
do not turn them into sampled or softened shortcuts.

The old RustGPU-to-WGSL path had captured-storage-pointer, nested-`continue`,
`Option` range-result and inclusive-loop failures on Metal. That translation
path is historical, not the current architecture. Its lasting lesson is to keep
storage indexing checked and traversal/results explicit, and verify behavior
with hardware oracles rather than trusting source equivalence or Naga alone.

Lens diagnostics need physically meaningful off-axis controls: a centered point
need not deform. Foreground-inverse changes must preserve far-only output; check
near/far directionality rather than demanding distinct hashes for every knob.
Clipped apertures normalize per destination. Keep independent PSF/energy/alpha
checks; a Legacy difference establishes neither correctness nor a regression.
Historical CPU stripe rounding differed by about 1.4e-5; that is not a substitute
for the required hardware full/ROI/stripe gate.
