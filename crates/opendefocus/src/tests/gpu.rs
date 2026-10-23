//! Hardware tests that compare GPU renders with the shared adaptive kernel and the aperture oracles.
use super::*;
use crate::datamodel::{
    Settings, UVector2,
    defocus::DefocusMode,
    render::{FilterMode, Quality, RenderSpecs},
};
use image_ndarray::prelude::ImageArray;
use ndarray::{Array2, Array3};

#[cfg(feature = "wgpu")]
/// Builds the 8 by 8 by 4 striped test aperture: ones in rows 2 to 5 for alternating column pairs, zeros elsewhere.
fn aperture_test_filter() -> Array3<f32> {
    Array3::from_shape_fn((8, 8, 4), |(y, x, c)| {
        if (x + c) % 4 <= 1 && y > 1 && y < 6 {
            1.0
        } else {
            0.0
        }
    })
}

#[cfg(feature = "wgpu")]
/// Computes the leaf oracle value of output texel (`x`, `y`) for `channel`: the weight-normalised source over the custom aperture, asserting that some source pixel contributes.
fn leaf_oracle_value(
    source: &Array3<f32>,
    coc: &Array2<f32>,
    sample_mip: &impl Fn(usize, glam::Vec2, usize) -> f32,
    mip_count: usize,
    (width, height): (usize, usize),
    (x, y): (usize, usize),
    channel: usize,
) -> f32 {
    let resolution = glam::Vec2::new(24.0, 12.0);
    let mut weighted_color = 0.0;
    let mut weight_sum = 0.0;
    for (source_y, source_x) in (0..height).flat_map(|y| (0..width).map(move |x| (y, x))) {
        let radius = coc[[source_y, source_x]].abs();
        let diameter = (radius * 2.0).max(1.0);
        let distance = glam::Vec2::new(source_x as f32 - x as f32, source_y as f32 - y as f32);
        let coordinates = -distance * (resolution / diameter) + resolution * 0.5;
        let uv = coordinates / resolution;
        if ((uv - glam::Vec2::splat(0.5)) * 2.0).length_squared() >= 1.0 {
            continue;
        }
        let max_scale = (glam::Vec2::splat(diameter) / resolution).max_element();
        let lod = (1.0 / max_scale).log2().max(0.0);
        let lower_mip = (lod.floor() as usize).min(mip_count - 1);
        let upper_mip = (lod.ceil() as usize).min(mip_count - 1);
        let fraction = lod - lod.floor();
        let aperture = sample_mip(lower_mip, uv, channel) * (1.0 - fraction)
            + sample_mip(upper_mip, uv, channel) * fraction;
        let weight = aperture / (radius * radius).max(0.25);
        weighted_color += source[[source_y, source_x, channel]] * weight;
        weight_sum += weight;
    }
    assert!(weight_sum > 0.0, "oracle has no support at ({x}, {y})");
    weighted_color / weight_sum
}

#[cfg(feature = "wgpu")]
/// Computes the aperture oracle at texel (0, 0): per channel, the normalised colour and the weight sum over a 4 by 4 window of the filter.
fn aperture_oracle_weights(
    prepared_filter: &Array3<f32>,
    channels: usize,
    inverse_foreground: bool,
) -> (Vec<f32>, Vec<f32>) {
    let inverse_shift = if inverse_foreground { 1.0 } else { 0.0 };
    let mut expected_channels = Vec::with_capacity(channels);
    let mut expected_weight_sums = Vec::with_capacity(channels);
    for channel in 0..channels {
        let mut weighted_color = 0.0;
        let mut weight_sum = 0.0;
        for y in 0..4 {
            for x in 0..4 {
                let shifted_x = x as f32 + inverse_shift;
                let shifted_y = y as f32 + inverse_shift;
                if shifted_x * shifted_x + shifted_y * shifted_y >= 16.0 {
                    continue;
                }
                // The 2D mode has positive CoC. The default
                // inverse-foreground flag shifts both filter
                // axes back by one filter texel.
                let texel_x = 3.5 - inverse_shift - x as f32;
                let texel_y = 3.5 - inverse_shift - y as f32;
                let x0 = texel_x.floor() as i32;
                let y0 = texel_y.floor() as i32;
                let frac_x = texel_x - x0 as f32;
                let frac_y = texel_y - y0 as f32;
                let sample_mask = |px: i32, py: i32| {
                    let px = px.clamp(0, 7) as usize;
                    let py = py.clamp(0, 7) as usize;
                    prepared_filter[(py, px, channel)]
                };
                let top = sample_mask(x0, y0) * (1.0 - frac_x) + sample_mask(x0 + 1, y0) * frac_x;
                let bottom =
                    sample_mask(x0, y0 + 1) * (1.0 - frac_x) + sample_mask(x0 + 1, y0 + 1) * frac_x;
                let aperture = top * (1.0 - frac_y) + bottom * frac_y;
                let weight = aperture / 16.0;
                let color = if channel == 3 {
                    0.8
                } else {
                    ((x * 3 + y * 5 + channel) % 17) as f32 / 16.0
                };
                weighted_color += color * weight;
                weight_sum += weight;
            }
        }
        let expected = weighted_color / weight_sum;
        expected_channels.push(expected);
        expected_weight_sums.push(weight_sum);
    }
    (expected_channels, expected_weight_sums)
}

#[cfg(feature = "wgpu")]
#[tokio::test]
#[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
/// Hardware check that renders match the shared adaptive kernel.
async fn test_gpu_renderings_match_shared_adaptive_kernel() {
    for channels in [3, 4] {
        for image_aperture in [false, true] {
            let source = Array3::from_shape_fn((16, 16, channels), |(y, x, c)| {
                if c == 3 {
                    0.8
                } else {
                    ((x * 3 + y * 5 + c) % 17) as f32 / 16.0
                }
            });
            let mut gpu_image = source.clone();
            let mut repeated_image = source;
            let mut settings = Settings::default();
            settings.defocus.circle_of_confusion.size = 4.0;
            settings.render.gui = true;
            settings.render.quality = Quality::Accurate as i32;
            let filter = image_aperture.then(aperture_test_filter);
            let prepared_filter = filter.as_ref().map(|filter| {
                crate::renders::filter::prepare_filter_image(
                    Some(filter.clone()),
                    &settings,
                    settings.defocus.get_padding().max(8),
                    4,
                )
                .unwrap()
            });
            if image_aperture {
                settings.render.filter.mode = FilterMode::Image as i32;
            }
            let inverse_foreground = settings.non_uniform.inverse_foreground;
            let gpu = OpenDefocusRenderer::new(&mut settings).await.unwrap();
            let repeated = OpenDefocusRenderer::new(&mut settings).await.unwrap();
            gpu.render(settings.clone(), gpu_image.view_mut(), None, filter.clone())
                .await
                .unwrap();
            repeated
                .render(settings, repeated_image.view_mut(), None, filter)
                .await
                .unwrap();
            if image_aperture {
                let (expected_channels, _) = aperture_oracle_weights(
                    prepared_filter.as_ref().unwrap(),
                    channels,
                    inverse_foreground,
                );
                for (channel, expected) in expected_channels.into_iter().enumerate() {
                    assert!(
                        (gpu_image[[0, 0, channel]] - expected).abs() < 1e-3,
                        "GPU aperture oracle at (0,0) channel {channel}: {} != {expected}",
                        gpu_image[[0, 0, channel]]
                    );
                    assert!(
                        (repeated_image[[0, 0, channel]] - expected).abs() < 1e-3,
                        "Repeated GPU aperture oracle at (0,0) channel {channel}: {} != {expected}",
                        repeated_image[[0, 0, channel]]
                    );
                }
            }
            for (index, (actual, expected)) in
                gpu_image.iter().zip(repeated_image.iter()).enumerate()
            {
                let x = index % (16 * channels) / channels;
                let y = index / (16 * channels);
                let channel = index % channels;
                assert!(
                    (actual - expected).abs() < 1e-3,
                    "channels={channels} image_aperture={image_aperture} at ({x},{y}) channel {channel}: {actual} != {expected}"
                );
            }
        }
    }
}

#[cfg(feature = "wgpu")]
#[tokio::test]
#[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
/// Hardware check that a custom aperture with negative and fractional LOD matches the oracle.
async fn test_gpu_custom_aperture_negative_and_fractional_lod_oracle() {
    for (name, signed_radius) in [
        ("negative direct CoC", -4.0_f32),
        ("fractional LOD", 2.5_f32),
    ] {
        let source = Array3::from_shape_fn((16, 16, 4), |(y, x, c)| {
            if c == 3 {
                0.8
            } else {
                ((x * 3 + y * 5 + c) % 17) as f32 / 16.0
            }
        });
        let mut settings = Settings::default();
        settings.render.resolution = UVector2 { x: 16, y: 16 };
        settings.render.gui = true;
        settings.render.quality = Quality::Accurate as i32;
        settings.render.filter.mode = FilterMode::Image as i32;
        settings.defocus.circle_of_confusion.size = signed_radius.abs();
        settings.defocus.circle_of_confusion.max_size = 4.0;
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.use_direct_math = true;

        let filter = Array3::from_shape_fn((8, 8, 4), |(y, x, c)| {
            if (x + c) % 4 <= 1 && y > 1 && y < 6 {
                1.0
            } else {
                0.0
            }
        });
        let prepared_filter = crate::renders::filter::prepare_filter_image(
            Some(filter.clone()),
            &settings,
            settings.defocus.get_padding().max(8),
            4,
        )
        .unwrap();
        let filter_mips = crate::renders::resize::create_mipmaps(&prepared_filter).unwrap();
        let mut depth = Some(Array2::from_elem((16, 16), signed_radius));
        let mut gpu_image = source;
        let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
        renderer
            .render(
                settings.clone(),
                gpu_image.view_mut(),
                depth.take(),
                Some(filter),
            )
            .await
            .unwrap();

        let resolution = 8.0;
        let diameter = (signed_radius.abs() * 2.0).max(1.0);
        let lod = (1.0 / (diameter / resolution)).log2().max(0.0);
        let lower_mip = (lod.floor() as usize).min(filter_mips.len() - 1);
        let upper_mip = (lod.ceil() as usize).min(filter_mips.len() - 1);
        let lod_fraction = lod - lod.floor();
        let sample = |mip: usize, uv: glam::Vec2, channel: usize| {
            let image = &filter_mips[mip];
            let (height, width, _) = image.dim();
            let texel = uv * glam::Vec2::new(width as f32, height as f32) - glam::Vec2::splat(0.5);
            let x0 = texel.x.floor() as i32;
            let y0 = texel.y.floor() as i32;
            let fraction = texel - glam::Vec2::new(x0 as f32, y0 as f32);
            let load = |x: i32, y: i32| {
                image[(
                    y.clamp(0, height as i32 - 1) as usize,
                    x.clamp(0, width as i32 - 1) as usize,
                    channel,
                )]
            };
            let top = load(x0, y0) * (1.0 - fraction.x) + load(x0 + 1, y0) * fraction.x;
            let bottom = load(x0, y0 + 1) * (1.0 - fraction.x) + load(x0 + 1, y0 + 1) * fraction.x;
            top * (1.0 - fraction.y) + bottom * fraction.y
        };

        for (destination_y, destination_x, channel) in (0..16)
            .flat_map(|y| (0..16).flat_map(move |x| (0..4).map(move |channel| (y, x, channel))))
        {
            let mut weighted_color = 0.0;
            let mut weight_sum = 0.0;
            for (source_y, source_x) in (0..16).flat_map(|y| (0..16).map(move |x| (y, x))) {
                let distance = glam::Vec2::new(
                    source_x as f32 - destination_x as f32,
                    source_y as f32 - destination_y as f32,
                );
                let mut coordinates =
                    -distance * (resolution / diameter) + glam::Vec2::splat(resolution * 0.5);
                if signed_radius > 0.0 && settings.non_uniform.inverse_foreground {
                    coordinates -= glam::Vec2::ONE;
                }
                let uv = coordinates / resolution;
                if ((uv.x - 0.5) * 2.0).powi(2) + ((uv.y - 0.5) * 2.0).powi(2) >= 1.0 {
                    continue;
                }
                let lower = sample(lower_mip, uv, channel);
                let upper = sample(upper_mip, uv, channel);
                let aperture = lower * (1.0 - lod_fraction) + upper * lod_fraction;
                let weight = aperture / signed_radius.abs().powi(2).max(0.25);
                let color = if channel == 3 {
                    0.8
                } else {
                    ((source_x * 3 + source_y * 5 + channel) % 17) as f32 / 16.0
                };
                weighted_color += color * weight;
                weight_sum += weight;
            }
            assert!(
                weight_sum > 0.0,
                "oracle has no support for {name} at ({destination_x},{destination_y}) channel {channel}"
            );
            let expected = weighted_color / weight_sum;
            assert!(
                (gpu_image[[destination_y, destination_x, channel]] - expected).abs() < 1e-3,
                "GPU aperture oracle {name} at ({destination_x},{destination_y}) channel {channel}: {} != {expected} (lod={lod})",
                gpu_image[[destination_y, destination_x, channel]]
            );
        }
    }
}

#[cfg(feature = "wgpu")]
#[tokio::test]
#[ignore = "requires a hardware GPU with FLOAT32_FILTERABLE"]
/// Hardware check that a custom aperture with variable radii matches the leaf oracle.
async fn test_gpu_custom_aperture_variable_radii_match_leaf_oracle() {
    let (width, height) = (16usize, 16usize);
    let max_radius = 3.5;
    let source = Array3::from_shape_fn((height, width, 4), |(y, x, channel)| {
        let alpha = 0.45 + ((x + 2 * y) % 5) as f32 * 0.1;
        if channel == 3 {
            alpha
        } else {
            (0.05 + (x * 3 + y * 5 + channel) as f32 / 200.0) * alpha
        }
    });
    let coc = Array2::from_shape_fn((height, width), |(y, x)| {
        -(2.5 + ((x + 2 * y) % 3) as f32 * 0.5)
    });
    let filter_mips = crate::renders::resize::create_mipmaps(&Array3::from_shape_fn(
        (12, 24, 4),
        |(y, x, channel)| {
            let edge = (1.0
                - ((x as f32 / 23.0 - 0.5).powi(2) + (y as f32 / 11.0 - 0.5).powi(2)).sqrt() * 2.0)
                .clamp(0.0, 1.0);
            edge * (0.25 + ((x * 3 + y * 5 + channel * 7) % 13) as f32 / 16.0)
        },
    ))
    .unwrap();
    let aperture = crate::renders::resize::MipmapBuffer::from_vec(filter_mips.clone()).unwrap();
    let runner = WgpuRunner::new_adaptive()
        .await
        .expect("hardware GPU required");
    let prepared = runner
        .prepare_adaptive_layers(&source, &coc, max_radius, CocPolarity::PositiveForeground)
        .await
        .unwrap();
    let features = runner
        .classify_tiles(&source, &coc, [0, 0], max_radius)
        .await
        .unwrap();
    let specs = RenderSpecs::from_rects(
        glam::IVec4::new(0, 0, width as i32, height as i32),
        glam::IVec4::new(0, 0, width as i32, height as i32),
    );
    let filter_resolution = glam::UVec2::new(24, 12);
    let sample_budget = AdaptiveSampleBudget::for_quality(Quality::Accurate);
    let sample_mip = |mip: usize, uv: glam::Vec2, channel: usize| {
        let image = &filter_mips[mip];
        let (mip_height, mip_width, _) = image.dim();
        let texel =
            uv * glam::Vec2::new(mip_width as f32, mip_height as f32) - glam::Vec2::splat(0.5);
        let origin = glam::IVec2::new(texel.x.floor() as i32, texel.y.floor() as i32);
        let fraction = texel - origin.as_vec2();
        let load = |x: i32, y: i32| {
            image[[
                y.clamp(0, mip_height as i32 - 1) as usize,
                x.clamp(0, mip_width as i32 - 1) as usize,
                channel,
            ]]
        };
        let top = load(origin.x, origin.y) * (1.0 - fraction.x)
            + load(origin.x + 1, origin.y) * fraction.x;
        let bottom = load(origin.x, origin.y + 1) * (1.0 - fraction.x)
            + load(origin.x + 1, origin.y + 1) * fraction.x;
        top * (1.0 - fraction.y) + bottom * fraction.y
    };

    for inverse_foreground in [false, true] {
        let mut settings = Settings::default();
        settings.defocus.set_defocus_mode(DefocusMode::Depth);
        settings.defocus.circle_of_confusion.max_size = max_radius;
        settings.render.resolution = UVector2 {
            x: width as u32,
            y: height as u32,
        };
        settings.render.filter.mode = FilterMode::Image as i32;
        settings.non_uniform.inverse_foreground = inverse_foreground;
        let lens = crate::datamodel::settings_to_convolve_settings(
            &settings,
            &specs,
            filter_resolution,
            (width * height) as u32,
        );
        let gpu_image = runner
            .gather_adaptive_frame_with_aperture(
                &source,
                &prepared,
                &features,
                max_radius,
                sample_budget,
                &lens,
                &aperture,
            )
            .await
            .unwrap();
        for (y, x) in (0..height).flat_map(|y| (0..width).map(move |x| (y, x))) {
            let mut expected = [0.0; 4];
            for channel in 0..4 {
                expected[channel] = leaf_oracle_value(
                    &source,
                    &coc,
                    &sample_mip,
                    filter_mips.len(),
                    (width, height),
                    (x, y),
                    channel,
                );
            }
            for channel in 0..4 {
                assert!(
                    (gpu_image[[y, x, channel]] - expected[channel]).abs() < 1e-3,
                    "GPU custom-aperture leaf oracle inverse_foreground={inverse_foreground} at ({x}, {y}) channel {channel}: {} != {}",
                    gpu_image[[y, x, channel]],
                    expected[channel],
                );
            }
        }
    }
}

#[tokio::test]
/// Hardware check of renders with multiple channels.
async fn test_multi_channel_renderings() {
    let test_image =
        image::load_from_memory(include_bytes!("../../../../test/images/any/toad.png"))
            .unwrap()
            .to_rgb32f();
    let mut settings = Settings::default();
    let renderer = OpenDefocusRenderer::new(&mut settings).await.unwrap();
    renderer
        .render(settings, test_image.to_ndarray().view_mut(), None, None)
        .await
        .unwrap();
}
