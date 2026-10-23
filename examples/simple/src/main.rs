//! Minimal example that renders the bundled toad image with OpenDefocus and
//! writes the result to `result.png` in the working directory.

use anyhow::Result;
use image::{DynamicImage, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
use opendefocus::OpenDefocusRenderer;
use opendefocus::datamodel::Settings;

/// Renders `toad.png` with a 25 pixel circle-of-confusion radius and saves `result.png`.
///
/// # Errors
///
/// Returns an error if the renderer cannot be initialized, the image cannot be
/// decoded or converted, or `result.png` cannot be written.
#[tokio::main]
async fn main() -> Result<()> {
    let mut settings = Settings::default();
    settings.defocus.circle_of_confusion.size = 25.0;

    // Reuse the renderer across images: creating it initializes the WGPU runner.
    let renderer = OpenDefocusRenderer::new(&mut settings).await?;

    let image = image::load_from_memory(include_bytes!("../toad.png"))?.to_rgba32f();
    let mut array = image.to_ndarray();

    renderer
        .render(settings, array.view_mut(), None, None)
        .await?;

    let image: Rgba32FImage = Rgba32FImage::from_ndarray(array)?;
    let image = DynamicImage::from(image).to_rgba8();
    image.save("./result.png")?;
    Ok(())
}
