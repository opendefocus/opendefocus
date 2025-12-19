# <i class="fa-solid fa-bullseye"></i> Defocus size

Besides the regular `size` knob there is also the `maximum` knob (a.k.a. max size). Understanding and using it properly can help get better results.

> [!NOTE]
> This is only important for depth based defocus, as 2D defocus will have the `max size` set to the regular `size`.
>
> Camera based defocus uses another algorithm, which clamps at the max size.

## Understanding the maximum knob
The regular `size` knob will set the size of the defocus in pixels when something is out of focus, in the far field.

The `maximum` controls two things simultaneously:

1. The maximum sets the maximum defocus size across the entire image. This is a clamping value.
2. The maximum defines the defocus size in the near field.

### Example
If the `size` knob is set to 5, the defocus size in the far field will be 5. If the `maximum` knob is set to 5, it will also be 5 in the near field. Lowering this value would clamp the size for the entire image.

However, increasing the `maximum` knob, for example to 10, will result in a defocus size of 10 in the near field.



<video src="../assets/max_size.mp4" controls="controls" width="100%" loop></video>
> This is shown in this following video. First, size and maximum are set to the same value, resulting in the same defocus across the image. Then: maximum is increased and shows that the near field also increases.
