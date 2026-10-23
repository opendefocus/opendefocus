# <i class="fa-solid fa-circle-half-stroke"></i> Inverse foreground
The **Inverse Foreground** parameter (Non-Uniform tab) is on by default. It applies the non-uniform effects inverted for the foreground. This is caused by the way light rays work physically.

The **Inverse** checkbox on Catseye and Barndoors is a separate, per-effect option.

---

## Visual explanation
When adjusting the focus, you are adjusting the position where lights intersect, and form a sharp image at that location.

The point where the lines collide is the point of focus. Notice that once they collided, they still continue in a straight line. This causes the light rays to be in an inverse order.

This is exactly the reason why optical artifacts inverse in the foreground in contrast to the far field: the light rays are in inversed order.

## Example image
When **Inverse Foreground** is on, the shape is inversed in the foreground, in the opposite direction of the background:

![Foreground focus](../../assets/fg_focus.jpg)
![Background focus](../../assets/bg_focus.jpg)
> Background and foreground in focus
