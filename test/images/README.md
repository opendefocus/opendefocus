# Test images

These images were created by the OpenDefocus project author and are included
with their permission for testing and evaluating OpenDefocus.

The standalone application's **Test images** menu embeds:

- `any/toad.png`: a 512 × 512 detail image.
- `resize/0.png`: a 256 × 256 color-bar chart.
- `preprocessor/image.jpg` and `preprocessor/depth.exr`: a matching
  1920 × 1080 foreground/background occlusion scene. The red channel of the EXR
  stores signed defocus radii, ranging from approximately -0.367 to 10 pixels;
  use Direct math with a maximum radius of at least 10 pixels.

These are the original fixtures; the application does not modify or duplicate
them. No external image downloads are needed.

`scenes/` adds four original, CC0 analytic HDR/depth/alpha diagnostic scenes.
See [the scene gallery and regeneration guide](scenes/README.md). Existing
fixtures above retain their original provenance and are preserved unchanged.
