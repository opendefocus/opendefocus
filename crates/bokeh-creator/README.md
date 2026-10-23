[![Tests](https://github.com/opendefocus/opendefocus/actions/workflows/test-bokeh-creator.yaml/badge.svg)](https://github.com/opendefocus/opendefocus/actions/workflows/test-bokeh-creator.yaml)
[![License](https://img.shields.io/crates/l/bokeh-creator)](https://crates.io/crates/bokeh-creator)
[![Version](https://img.shields.io/crates/v/bokeh-creator)](https://crates.io/crates/bokeh-creator)


# Bokeh Creator

Simple library to replicate real world lens kernels written in Rust. Currently it requires the `std` library.

## Install
Add this crate to your project by adding it in your `Cargo.toml`:
```bash
cargo add bokeh-creator
```

The [Image](https://crates.io/crates/image) crate is optional, as there is a method to render when you specify coordinates only.


## Usage

```rust
use bokeh_creator::{Renderer, Settings};
use image::Rgba32FImage;

fn main() {
    let Settings = Settings {
        angle: 195.3,
        curvature: 0.1,
        ..Default::default()
    };
    let mut result = Rgba32FImage::new(256, 256);
    let renderer = Renderer::render_to_image(&mut result, Settings);
    // Do whatever you need to do with the result :)
}
```


## Examples

![Expected render 1](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/1_expected.jpg)
```rust
use bokeh_creator::Settings;
Settings::default();
```

![Expected render 2](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/2_expected.jpg)
```rust
use bokeh_creator::{FilterType, Settings};
Settings {
    filter_type: FilterType::Blade.into(),
    angle: 195.3,
    curvature: 0.1,
    ..Default::default()
};
```

![Expected render 3](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/3_expected.jpg)
```rust
use bokeh_creator::{FilterType, Settings};
Settings {
    filter_type: FilterType::Blade.into(),
    angle: 90.0,
    blades: 15,
    ..Default::default()
};
```


![Expected render 4](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/4_expected.jpg)
```rust
use bokeh_creator::Settings;
Settings {
    aspect_ratio: 0.5,
    ..Default::default()
};
```

![Expected render 5](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/5_expected.jpg)
```rust
use bokeh_creator::Settings;
Settings {
    aspect_ratio: 2.0,
    ..Default::default()
};
```

![Expected render 6](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/6_expected.jpg)
```rust
use bokeh_creator::Settings;
Settings {
    ring_color: 0.5,
    inner_color: 0.9,
    ring_size: 0.5,
    ..Default::default()
};
```

![Expected render 7](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/7_expected.jpg)
```rust
use bokeh_creator::{Noise, Settings};
Settings {
    noise: {
        Noise {
            size: 0.3,
            intensity: 1.0,
            ..Default::default()
        }
    },
    ..Default::default()
};
```

![Expected render 8](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/8_expected.jpg)
```rust
use bokeh_creator::{Noise, Settings};
Settings {
    noise: {
        Noise {
            intensity: 0.0,
            ..Default::default()
        }
    },
    ..Default::default()
};
```

![Expected render 9](https://raw.githubusercontent.com/opendefocus/opendefocus/main/test/images/bokeh-creator/9_expected.jpg)
```rust
use bokeh_creator::{Noise, Settings};
Settings {
    noise: {
        Noise {
            seed: 30,
            ..Default::default()
        }
    },
    ..Default::default()
};
```
