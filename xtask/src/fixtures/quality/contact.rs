//! Contact sheet of the adaptive preview images produced by a QA run.
use std::path::{Path, PathBuf};

use anyhow::Result;
use image::{DynamicImage, RgbImage};
use serde_json::{Map, Value, json};

/// Number of thumbnails per row in the contact sheet.
const COLUMNS: u32 = 4;
/// Width in pixels of each thumbnail cell.
const CELL_WIDTH: u32 = 300;
/// Height in pixels of each thumbnail cell.
const CELL_HEIGHT: u32 = 210;
/// Background colour of the sheet, behind the thumbnails.
const BACKGROUND: [u8; 3] = [0x20, 0x20, 0x20];

/// Tile every `adaptive-*.preview.png` into one sheet and report its path.
///
/// Unlike the Python reference this sheet has no per-cell labels and no motion GIF:
/// the image crate in xtask has no font or GIF encoder enabled.
pub fn export_visual_review(output: &Path) -> Result<Map<String, Value>> {
    let mut result = Map::new();
    let previews = matching_previews(output, "adaptive-", ".preview.png")?;
    if previews.is_empty() {
        return Ok(result);
    }
    let rows = (previews.len() as u32).div_ceil(COLUMNS);
    let mut sheet = RgbImage::from_pixel(
        COLUMNS * CELL_WIDTH,
        rows * CELL_HEIGHT,
        image::Rgb(BACKGROUND),
    );
    for (index, path) in previews.iter().enumerate() {
        let source = image::open(path)?.to_rgb8();
        let thumbnail = if source.width() <= CELL_WIDTH && source.height() <= CELL_HEIGHT {
            source
        } else {
            DynamicImage::ImageRgb8(source)
                .thumbnail(CELL_WIDTH, CELL_HEIGHT)
                .to_rgb8()
        };
        let column = index as u32 % COLUMNS;
        let row = index as u32 / COLUMNS;
        let x = column * CELL_WIDTH + (CELL_WIDTH - thumbnail.width()) / 2;
        let y = row * CELL_HEIGHT;
        image::imageops::replace(&mut sheet, &thumbnail, i64::from(x), i64::from(y));
    }
    let sheet_path = output.join("adaptive-contact-sheet.png");
    sheet.save(&sheet_path)?;
    result.insert(
        "contact_sheet".to_owned(),
        json!(sheet_path.display().to_string()),
    );
    Ok(result)
}

/// Files whose names start with `prefix` and end with `suffix`, sorted by path.
fn matching_previews(directory: &Path, prefix: &str, suffix: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        if name.is_some_and(|name| name.starts_with(prefix) && name.ends_with(suffix)) {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}
