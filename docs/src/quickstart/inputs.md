# <i class="fa-solid fa-box"></i> Inputs

The plugin has two inputs, and a third optional input in hosts that provide a camera.

1. **Source**: the image you want to defocus. Only RGBA is processed; the plugin does not select layers or channels. The Source is assumed to be premultiplied.
2. **Depth**: the depth map used in Depth mode. It can be RGBA or alpha, and the first channel is used. It must be connected in Depth mode and is not used in 2D mode.
3. **Camera** (optional): only shown in hosts that provide the Nuke camera extension. A connected camera supplies the focal length and filmback. See [<i class="fa-solid fa-video"></i> Camera Input](../detailed/camera_input.md).

The output is float RGBA, premultiplied. Errors, such as a missing Depth input in Depth mode, are written to the host's log.

---
