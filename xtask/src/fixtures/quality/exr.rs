//! Reads OpenEXR files through the external EXR dumper, which writes one raw float file per
//! channel.
use std::{fs, path::Path};

use anyhow::{Context, Result, bail};

use super::{image::Rgba, process};

/// Read an EXR through the external dumper, which writes one raw `PREFIX-CHANNEL.f32` per channel.
pub fn read_exr(exr_dump: &Path, path: &Path, prefix: &Path) -> Result<Rgba> {
    let arguments = [path.display().to_string(), prefix.display().to_string()];
    let output = process::capture(exr_dump, &arguments)?;
    if !output.status.success() {
        bail!(
            "EXR dumper failed for {}: {}",
            path.display(),
            output.status
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut sizes = stdout.split_whitespace().take(2).map(str::parse::<usize>);
    let (Some(Ok(width)), Some(Ok(height))) = (sizes.next(), sizes.next()) else {
        bail!("EXR dumper must print `WIDTH HEIGHT`, got {stdout:?}");
    };
    let channels = read_channels(prefix, width * height)?;
    let rgb = |name: &str| {
        channels
            .iter()
            .find(|(channel, _)| channel == name)
            .map(|(_, values)| values)
    };
    let (Some(red), Some(green), Some(blue)) = (rgb("r"), rgb("g"), rgb("b")) else {
        let mut names: Vec<&str> = channels.iter().map(|(name, _)| name.as_str()).collect();
        names.sort_unstable();
        bail!("EXR missing RGB: {names:?}");
    };
    let alpha = rgb("a");
    let data: Vec<f32> = (0..width * height)
        .flat_map(|index| {
            [
                red[index],
                green[index],
                blue[index],
                alpha.map_or(1.0, |values| values[index]),
            ]
        })
        .collect();
    if data.iter().any(|value| !value.is_finite()) {
        bail!("Non-finite EXR values: {}", path.display());
    }
    Ok(Rgba::new(width, height, data))
}

/// Read and delete every `PREFIX-NAME.f32` channel file next to the prefix.
fn read_channels(prefix: &Path, pixels: usize) -> Result<Vec<(String, Vec<f32>)>> {
    let directory = prefix.parent().context("EXR prefix has no parent")?;
    let stem = prefix
        .file_name()
        .context("EXR prefix has no file name")?
        .to_string_lossy()
        .into_owned();
    let mut channels = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        let file = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned());
        let Some(file) = file else { continue };
        let Some(middle) = file
            .strip_prefix(&format!("{stem}-"))
            .and_then(|rest| rest.strip_suffix(".f32"))
        else {
            continue;
        };
        let bytes = fs::read(&path)?;
        fs::remove_file(&path)?;
        if bytes.len() != pixels * 4 {
            bail!("Bad EXR channel dimensions: {}", path.display());
        }
        let values = bytes
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| f32::from_le_bytes(*chunk))
            .collect();
        channels.push((middle.to_lowercase(), values));
    }
    Ok(channels)
}
