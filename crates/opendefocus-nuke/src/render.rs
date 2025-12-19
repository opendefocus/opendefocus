use anyhow::{Error, Result};
use ndarray::{Array2, Array3, ArrayViewMut3};
use opendefocus::{OpenDefocusRenderer, datamodel::render::FilterMode};

pub async fn render_convolution(
    settings: opendefocus::datamodel::Settings,
    renderer: OpenDefocusRenderer,
    image: &mut [f32],
    use_depth: bool,
    depth: &[f32],
    channels: usize,
    full_region: [i32; 4],
    render_region: [i32; 4],
    filter: &[f32],
    filter_channels: usize,
    filter_region: [i32; 2],
) -> Result<()> {
    if image.is_empty() {
        return Err(Error::msg("Provided image is null."));
    }
    let render_specs = opendefocus::datamodel::render::RenderSpecs {
        full_region: opendefocus::datamodel::IVector4 {
            x: full_region[0],
            y: full_region[1],
            z: full_region[2],
            w: full_region[3],
        },
        render_region: opendefocus::datamodel::IVector4 {
            x: render_region[0],
            y: render_region[1],
            z: render_region[2],
            w: render_region[3],
        },
    };
    let resolution = render_specs.get_resolution();
    let mut image = ArrayViewMut3::from_shape(
        (
            resolution.y as usize,
            resolution.x as usize,
            channels as usize,
        ),
        image,
    )?;

    if use_depth && depth.is_empty() {
        return Err(Error::msg("Rendering depth but no depth data provided."));
    }

    let depth_array = if use_depth {
        Array2::from_shape_vec(
            (resolution.y as usize, resolution.x as usize),
            depth.to_vec(),
        )?
    } else {
        Array2::zeros((1, 1))
    };

    if settings.render.filter.mode() == FilterMode::Image && filter.is_empty() {
        return Err(Error::msg("Rendering depth but no depth data provided."));
    }

    let filter_image = if settings.render.filter.mode() == FilterMode::Image {
        Some(Array3::from_shape_vec(
            (
                filter_region[1] as usize,
                filter_region[0] as usize,
                filter_channels,
            ),
            filter.to_vec(),
        )?)
    } else {
        None
    };

    renderer
        .render(
            render_specs,
            settings,
            &mut image,
            depth_array,
            filter_image,
        )
        .await?;

    Ok(())
}
