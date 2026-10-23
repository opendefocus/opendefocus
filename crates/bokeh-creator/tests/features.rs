//! Integration tests for bokeh settings, the renderer and its image-array paths.

use bokeh_creator::{FilterType, Noise, Settings};

#[test]
fn test_settings_clone_roundtrip_preserves_settings() {
    let settings = Settings {
        filter_type: FilterType::Blade.into(),
        blades: 7,
        angle: 123.0,
        noise: Noise {
            seed: 30,
            ..Default::default()
        },
        ..Default::default()
    };
    let cloned = settings;
    assert_eq!(cloned, settings);
    assert_eq!(
        FilterType::try_from(cloned.filter_type),
        Ok(FilterType::Blade)
    );
}

#[cfg(feature = "documented")]
#[test]
fn test_documented_fields_used_by_consumers() {
    use documented::DocumentedFields;
    for field in [
        "ring_color",
        "inner_color",
        "ring_size",
        "outer_blur",
        "inner_blur",
        "blades",
        "angle",
        "curvature",
        "aspect_ratio",
    ] {
        assert_ne!(Settings::get_field_docs(field).unwrap(), "");
    }
    for field in ["size", "intensity", "seed"] {
        assert_ne!(Noise::get_field_docs(field).unwrap(), "");
    }
}

#[cfg(feature = "renderer")]
#[test]
fn test_scalar_renderer_is_deterministic_and_finite() {
    use bokeh_creator::Renderer;
    use glam::USizeVec2;
    let settings = Settings::default();
    let a = Renderer::new(settings, USizeVec2::new(32, 32));
    let b = Renderer::new(settings, USizeVec2::new(32, 32));
    for y in 0..32 {
        for x in 0..32 {
            let coordinate = USizeVec2::new(x, y);
            let value = a.render_pixel(coordinate, 0);
            assert!(value.is_finite());
            assert_eq!(value.to_bits(), b.render_pixel(coordinate, 0).to_bits());
        }
    }
}

#[cfg(feature = "image")]
#[test]
fn test_image_array_and_scalar_paths_agree() {
    use bokeh_creator::Renderer;
    use glam::USizeVec2;
    use image::Rgba32FImage;
    use ndarray::Array3;
    for (width, height) in [(32, 32), (17, 32), (32, 17)] {
        let settings = Settings {
            filter_type: FilterType::Blade.into(),
            angle: 195.3,
            ..Default::default()
        };
        let mut image = Rgba32FImage::new(width, height);
        let mut array = Array3::<f32>::zeros((height as usize, width as usize, 4));
        Renderer::render_to_image(&mut image, settings);
        Renderer::render_to_array(settings, &mut array.view_mut());
        let resolution = width.max(height) as usize;
        let renderer = Renderer::new(settings, USizeVec2::new(resolution, resolution));
        for ((y, x, channel), value) in array.indexed_iter() {
            assert_eq!(
                value.to_bits(),
                image.get_pixel(x as u32, y as u32).0[channel].to_bits()
            );
            assert_eq!(
                value.to_bits(),
                renderer
                    .render_pixel(USizeVec2::new(x, y), channel)
                    .to_bits()
            );
        }
    }
}
