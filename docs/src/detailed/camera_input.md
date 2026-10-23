# <i class="fa-solid fa-video"></i> Camera Input
OpenDefocus supports a camera for optically accurate defocus. This means that according to the camera settings the defocus would match the result from real life. Using the camera is easy.

There is no need for any position data. If you want your defocus to match for example a 50mm lens, set **Focal Length** and the filmback, and set the **F-Stop** to your expected stop.

---

## Use Camera
Enable **Use Camera** on the Camera tab. The circle of confusion is then computed from the camera values: **Focal Length** (default 50 mm), **F-Stop** (default 16), **Filmback Width** and **Height** (default 24.576 x 18.672 mm). These rows have no effect while Use Camera is off, but they are not greyed out.

The camera applies in Depth mode with **Math** not set to **Direct**. In that case **Camera Max Size** (default 10) replaces **Max Size**.

---

## Camera input
In hosts that provide the Nuke camera extension, the plugin has an optional **Camera** input. When a camera is connected, Use Camera is forced on, and the focal length and filmback come from the camera. F-Stop and World Unit still come from their parameters. This input is untested in Nuke.

---

## World unit
It is important to set the **World Unit** the depth map was rendered in (`mm`, `cm`, `dm`, `m`, `in` or `ft`, default `m`). As each pixel in a depth map represents the distance, a value of 10 cm would be a completely different result as it would be if it is 1000 cm.

!!! warning annotate "Math"
    As with any depth based defocus, always make sure the **Math** is set correctly. Refer to [<i class="fa-solid fa-subscript"></i> Depth math](depth_math.md) for more information.

---

## Controlling the focal plane
The **Focal Plane** parameter (Defocus tab) sets the depth that stays in focus. When the camera is used, the focal plane must be a positive distance, in the **World Unit**. See [<i class="fa-solid fa-arrows-to-eye"></i> Focal plane](focal_plane_setup.md).

---

## I'd like to increase the size of the bokeh
Just as with any real camera, increase or decrease the `f`-stop to change the size of the bokeh. Lowering the **F-Stop** will make the bokeh larger, while increasing the **F-Stop** will decrease the bokeh size.

Make sure to set **Camera Max Size** to the size you want to define as a maximum, as no value will go higher than that.
