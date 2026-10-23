//! Image file input and output for the CLI: loading validated float RGBA arrays,
//! resolving paths for overwrite checks, and atomically saving images and reports.

use anyhow::{Context, Result, ensure};
use image::{DynamicImage, ImageFormat, Rgba32FImage};
use image_ndarray::prelude::ImageArray;
use ndarray::Array3;
use std::{
    io::{BufWriter, Write},
    path::{Path, PathBuf},
};

/// Loads an image as a height x width x 4 float RGBA array.
///
/// # Errors
///
/// Returns an error if the file cannot be opened or decoded, its width or height is zero
/// or exceeds `i32::MAX`, or any value is NaN or infinite.
pub fn load(path: &Path) -> Result<Array3<f32>> {
    let image = image::open(path).with_context(|| format!("Loading {}", path.display()))?;
    ensure!(
        image.width() > 0
            && image.height() > 0
            && i32::try_from(image.width()).is_ok()
            && i32::try_from(image.height()).is_ok(),
        "Invalid image dimensions"
    );
    let array = image.to_rgba32f().to_ndarray();
    ensure!(
        array.iter().all(|v| v.is_finite()),
        "{} contains NaN or infinity",
        path.display()
    );
    Ok(array)
}

/// Returns an absolute path for overwrite checks, so `./` aliases and symlinks compare equal.
///
/// Existing paths are canonicalized. For a missing path, its parent directory is
/// canonicalized and the file name is appended, so a missing input still fails later
/// with a loading error.
///
/// # Errors
///
/// Returns an error if the path or its parent cannot be resolved, or if it has no file name.
pub fn resolved_path(path: &Path) -> Result<PathBuf> {
    if path.exists() {
        return Ok(path.canonicalize()?);
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(parent
        .canonicalize()
        .with_context(|| format!("Resolving parent of {}", path.display()))?
        .join(path.file_name().context("Path must name a file")?))
}

/// Returns the image format implied by the file extension of `path`.
///
/// Only `OpenEXR`, `TIFF`, `HDR`, `PNG` and `JPEG` are accepted.
///
/// # Errors
///
/// Returns an error if the extension is unknown or the format is not supported.
pub fn output_format(path: &Path) -> Result<ImageFormat> {
    let format = ImageFormat::from_path(path).context("Unknown output extension")?;
    ensure!(
        matches!(
            format,
            ImageFormat::OpenExr
                | ImageFormat::Tiff
                | ImageFormat::Hdr
                | ImageFormat::Png
                | ImageFormat::Jpeg
        ),
        "Unsupported output format {format:?}"
    );
    Ok(format)
}

/// Encodes `output` in `format` and atomically replaces `path` with it.
///
/// Values are converted to the format's channel layout and bit depth first (PNG 16-bit,
/// JPEG RGB8, HDR RGB float). The data is written to a temporary file in the same
/// directory and then renamed, so a failed encode leaves an existing file untouched.
///
/// # Errors
///
/// Returns an error if any value is NaN or infinite, the array cannot become an image,
/// or encoding or writing fails.
pub fn save(path: &Path, format: ImageFormat, output: Array3<f32>) -> Result<()> {
    ensure!(
        output.iter().all(|v| v.is_finite()),
        "Renderer produced NaN or infinity; output was not saved"
    );
    let image = DynamicImage::from(Rgba32FImage::from_ndarray(output)?);
    let image = match format {
        ImageFormat::Png => DynamicImage::from(image.to_rgba16()),
        ImageFormat::Jpeg => DynamicImage::from(image.to_rgb8()),
        ImageFormat::Hdr => DynamicImage::from(image.to_rgb32f()),
        _ => image,
    };
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    {
        let mut writer = BufWriter::new(temporary.as_file_mut());
        image
            .write_to(&mut writer, format)
            .with_context(|| format!("Encoding {}", path.display()))?;
        writer
            .flush()
            .with_context(|| format!("Writing {}", path.display()))?;
    }
    temporary
        .persist(path)
        .with_context(|| format!("Saving {}", path.display()))?;
    Ok(())
}

/// Atomically writes `text` to `path` through a temporary file in the same directory.
///
/// # Errors
///
/// Returns an error if the temporary file cannot be created, written, or renamed to `path`.
pub fn save_report(path: &Path, text: &str) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(text.as_bytes())?;
    temporary
        .persist(path)
        .with_context(|| format!("Saving report {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod file_output_tests {
    use super::*;

    /// Checks that a failed encode leaves the existing output file and no temporary file behind.
    #[test]
    fn test_failed_encoding_preserves_existing_image() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("output.exr");
        std::fs::write(&path, "existing output").unwrap();
        let image = Array3::zeros((2, 2, 4));
        assert!(save(&path, ImageFormat::Bmp, image).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"existing output");
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }

    /// Checks that saving over hard links replaces the output without modifying the linked input.
    #[test]
    fn test_output_hard_links_do_not_change_input_files() {
        let directory = tempfile::tempdir().unwrap();
        let input = directory.path().join("input");
        std::fs::write(&input, "pristine input").unwrap();
        let output = directory.path().join("output.exr");
        let report = directory.path().join("report.json");
        std::fs::hard_link(&input, &output).unwrap();
        std::fs::hard_link(&input, &report).unwrap();
        save(&output, ImageFormat::OpenExr, Array3::ones((2, 2, 4))).unwrap();
        save_report(&report, "{\"complete\":true}").unwrap();
        assert_eq!(std::fs::read(input).unwrap(), b"pristine input");
        assert_eq!(
            image::open(output).unwrap().to_rgba32f().dimensions(),
            (2, 2)
        );
        assert_eq!(
            std::fs::read_to_string(report).unwrap(),
            "{\"complete\":true}"
        );
    }
}
