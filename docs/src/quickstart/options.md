# <i class="fa-solid fa-gears"></i> Options

The Defocus tab is responsible for the defocus settings: how the defocus is applied, its size, and the focal plane.

The most important parameters are:

* **Defocus Mode**: `2D` (the defocus is set by Size only) or `Depth` (the defocus is computed from the Depth input).
* **Math**: only in Depth mode, defines the type of depth map being used. Either `Real` or `1/Z` (default). ([<i class="fa-solid fa-subscript"></i> Depth math](../detailed/depth_math.md))
* **Size**: the size of the defocus in pixels (default 5).
* **Max Size**: in Depth mode, the maximum defocus value (default 10). ([<i class="fa-solid fa-bullseye"></i> Defocus size](../detailed/defocus_size.md))
* **Focal Plane**: the depth value that stays in focus (default 0). ([<i class="fa-solid fa-arrows-to-eye"></i> Focal plane](../detailed/focal_plane_setup.md))
* **Quality**: `Fast` or `Accurate` (default). ([<i class="fa-solid fa-star"></i> Quality](./quality.md))

The Defocus tab also has **Protect** (the width of the in-focus band around the focal plane), **Size Multiplier** and **Gamma**.
