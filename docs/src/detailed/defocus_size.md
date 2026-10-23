# <i class="fa-solid fa-bullseye"></i> Defocus size

Besides the regular **Size** parameter there is also **Max Size**, and **Size Multiplier** which scales both. Understanding and using them properly can help get better results.

> [!NOTE]
> This is only important for Depth mode. In 2D mode the defocus is **Size** times **Size Multiplier**.
>
> With Use Camera on, **Camera Max Size** is used in place of **Max Size**.

## Understanding Max Size
The regular **Size** parameter will set the size of the defocus in pixels when something is out of focus, in the far field.

**Max Size** controls two things simultaneously:

1. The maximum sets the maximum defocus size across the entire image. This is a clamping value.
2. The maximum defines the defocus size in the near field.

### Example
If **Size** is set to 5, the defocus size in the far field will be 5. If **Max Size** is set to 5, it will also be 5 in the near field. Lowering this value would clamp the size for the entire image.

However, increasing **Max Size**, for example to 10, will result in a defocus size of 10 in the near field.

<video src="../assets/max_size.mp4" controls="controls" width="100%" loop></video>
> This is shown in this following video. First, Size and Max Size are set to the same value, resulting in the same defocus across the image. Then: Max Size is increased and shows that the near field also increases.

## Limits
The effective radius is Max Size times Size Multiplier in Depth mode, and Size times Size Multiplier in 2D mode. Renders with an effective radius above 16384 px are rejected, and the error is written to the host's log.
