# opendefocus-kernel

This crate is the source code for the actual convolution calculation. The algorithm is based on a lot of trial and error, ideas from the presentation of [Guillaume Abadie "A Life of a Bokeh"](https://advances.realtimerendering.com/s2018/index.htm) and some ideas from the [AMD FidelityFX documentation](https://gpuopen.com/manuals/fidelityfx_sdk/techniques/depth-of-field/).

## (Extremely) Simplified explanation

The algorithm is ring based scatter and gather algorithm. The algorithm is sort of the other way of how you'd like to perform convolution. In an ideal world, with massive compute power, we would calculate the convolution disk for each pixel. So per pixel check how large it is, and draw them onto all these pixels in the canvas. HOWEVER, this is not possible unfortunately. How would you speed this up? As there would be lots of conflicts with simulatanious image write operations. Practically impossible, and not suitable for GPU based acceleration.

So, this algorithm is capable of calculating the value of the convolution of all pixels around it onto it's own pixel. It checks the entire region if neighbour pixels would overlap onto its pixel and adds it accordingly. At the same time also calculating the bokeh value and multiplying that with the pixel value of that source pixel.

We start from the largest ring possible. Does it overlay? Yes? Okay, then draw it onto the current process pixel. Once the ring is finished collection, we merge this current with with the previous ring. As rings closer to the smallest ring are closer to the camera, these closer rings can occlude the previous ring.

Once all ring collection is finished, we need to start the process again for the foreground.

After all this is done, we merge both fore- and background and we have a convoluted image.


### Details

I've left lots of details here, but check out the source code! Reading code gives you all the details.
