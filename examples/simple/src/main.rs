use anyhow::Result;
use image::{DynamicImage, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
use opendefocus::OpenDefocusRenderer;
use opendefocus::datamodel::Settings;

#[tokio::main]
async fn main() -> Result<()> {
    let mut settings = Settings::default();
    // set the defocus size in pixel radius, you can change whatever you want
    settings.defocus.circle_of_confusion.size = 25.0;

    // initialize a new renderer, this contains the runner instance (wgpu for example)
    // its fine to throw away if you only use one image, else its good to reuse to prevent initializing all the time
    let renderer = OpenDefocusRenderer::new(true, &mut settings).await?;

    // load an example image
    let image = image::load_from_memory(include_bytes!("../toad.png"))?.to_rgba32f();
    let mut array = image.to_ndarray();

    // then here we actually render
    renderer
        .render(settings, array.view_mut(), None, None)
        .await?;

    // just some loading to the image crate once again after rendering and storing it
    let image: Rgba32FImage = Rgba32FImage::from_ndarray(array)?;
    let image = DynamicImage::from(image).to_rgba8();
    image.save("./result.png")?;
    Ok(())
}
