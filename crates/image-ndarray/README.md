[![Tests](https://github.com/opendefocus/opendefocus/actions/workflows/test-image-ndarray.yaml/badge.svg)](https://github.com/opendefocus/opendefocus/actions/workflows/test-image-ndarray.yaml)
[![License](https://img.shields.io/crates/l/image-ndarray)](https://crates.io/crates/image-ndarray)
[![Version](https://img.shields.io/crates/v/image-ndarray)](https://crates.io/crates/image-ndarray)

# image-ndarray

Zero-copy views and standard-layout ownership conversions between `ImageBuffer` and ndarrays.
Non-standard array layouts are copied into image row order.

Working with ndarrays allows for easy manipulation of pixel values.

While there is another crate called `ndarray-image`, that works with dedicated types.
This crate does not and implements the methods directly onto the ImageBuffer objects.

## Usage
Add the crate to your project:
```bash
cargo add image-ndarray
```

Then in your project, when you want to use the ndarrays on the images, make sure to add the prelude:
```rust
use image_ndarray::prelude::*;
```

## Example

Adding a simple value:
```rust
#[cfg(feature = "image")]
{
    use image::Rgba32FImage;
    use image_ndarray::prelude::*;

    let mut my_image = Rgba32FImage::new(1920, 1080);

    assert!(my_image.to_vec() == vec![0.0; 1920 * 1080 * 4]);

    let mut array = my_image.as_ndarray_mut();
    array += 1.0;

    assert!(my_image.to_vec() == vec![1.0; 1920 * 1080 * 4]);
}
```

Adding another image:
```rust
#[cfg(feature = "image")]
{
    use image::Rgba32FImage;
    use image_ndarray::prelude::*;

    let mut my_image = Rgba32FImage::new(1920, 1080);
    let second_image = Rgba32FImage::from_vec(1920, 1080, vec![1.0; 1920 * 1080 * 4]).unwrap();
    assert!(my_image.to_vec() == vec![0.0; 1920 * 1080 * 4]);

    let mut array = my_image.as_ndarray_mut();
    array += &second_image.as_ndarray();

    assert!(my_image.to_vec() == vec![1.0; 1920 * 1080 * 4]);
}
```


Dividing another image (just any math operation supported by [ndarray](https://docs.rs/ndarray/latest/ndarray/struct.ArrayBase.html)):
```rust
#[cfg(feature = "image")]
{
    use image::Rgba32FImage;
    use image_ndarray::prelude::*;

    let mut my_image = Rgba32FImage::from_vec(1920, 1080, vec![1.0; 1920 * 1080 * 4]).unwrap();
    let second_image = Rgba32FImage::from_vec(1920, 1080, vec![2.0; 1920 * 1080 * 4]).unwrap();
    assert!(my_image.to_vec() == vec![1.0; 1920 * 1080 * 4]);

    let mut array = my_image.as_ndarray_mut();
    array /= &second_image.as_ndarray();

    assert!(my_image.to_vec() == vec![0.5; 1920 * 1080 * 4]);
}
```

Convert image to array:
```rust
#[cfg(feature = "image")]
{
    use image::Rgba32FImage;
    use image_ndarray::prelude::*;

    let my_image = Rgba32FImage::from_vec(1920, 1080, vec![1.0; 1920 * 1080 * 4]).unwrap();
    let array = my_image.to_ndarray();

    assert!(array.as_slice().unwrap().to_vec() == vec![1.0; 1920 * 1080 * 4]);
}
```


Or just create an image from the provided array :
```rust
#[cfg(feature = "image")]
{
    use image::Rgba32FImage;
    use image_ndarray::prelude::*;
    use ndarray::Array3;

    let array = Array3::from_elem((1080, 1920, 4), 1.0);
    let my_image = Rgba32FImage::from_ndarray(array).unwrap();

    assert!(my_image.to_vec() == vec![1.0; 1920 * 1080 * 4]);
}
```

And convert an u8 image into a f32 array:
```rust
use image_ndarray::prelude::*;
use ndarray::Array3;

let array = Array3::<u8>::from_elem((1080, 1920, 4), 255);
let float_array = array.map(|f| f.to_f32_normalized().unwrap());

let max_value = float_array.iter().cloned().fold(f32::MIN, f32::max);
assert!(max_value == 1.0);

```


## Workspace development

This workspace uses upstream 0.1.6 and ndarray 0.17. Tests use generated numeric data, with no external image fixtures.
