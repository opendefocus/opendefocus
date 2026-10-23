//! Writes one adaptive case's EXR inputs and describes it for the manifest.

use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::fixtures::scenes::{exr, exr::Rgba, json::Json};

/// How a case encodes its depth input: real depth in metres, inverse depth (1/Z), or a
/// signed circle of confusion given directly.
#[derive(Clone, Copy)]
pub enum Mode {
    /// Depth in metres; the renderer derives the CoC from it.
    RealDepth,
    /// Inverse depth, 1/Z.
    InverseDepth,
    /// Signed circle of confusion given directly.
    DirectCoc,
}

impl Mode {
    /// Mode name recorded in the manifest.
    fn name(self) -> &'static str {
        match self {
            Self::RealDepth => "real_depth",
            Self::InverseDepth => "inverse_depth",
            Self::DirectCoc => "direct_coc",
        }
    }

    /// CoC formula code recorded in the manifest: `0` for real depth and `1` for the direct
    /// formulas.
    fn coc_math(self) -> i64 {
        match self {
            Self::RealDepth => 0,
            Self::InverseDepth | Self::DirectCoc => 1,
        }
    }
}

/// Linear `(z, z, z, 1)` pixels; direct CoC maps use the same encoding.
pub fn depth_pixels(depth: &[f64]) -> Vec<Rgba> {
    depth
        .iter()
        .map(|&value| [value, value, value, 1.0])
        .collect()
}

/// Write `color.exr` and the depth or signed-CoC EXR, then return the manifest entry.
///
/// `extra` keys replace same-named base keys in place, as Python's `dict` merge does.
pub fn write_case(
    root: &Path,
    name: &str,
    scene: (&[Rgba], &[f64]),
    mode: Mode,
    dims: (usize, usize),
    extra: Vec<(&'static str, Json)>,
) -> Result<Json> {
    let (color, depth) = scene;
    let (width, height) = dims;
    let case_dir = root.join(name);
    fs::create_dir_all(&case_dir).with_context(|| format!("creating {}", case_dir.display()))?;
    let color_path = case_dir.join("color.exr");
    let depth_name = if matches!(mode, Mode::DirectCoc) {
        "signed-coc.exr"
    } else {
        "depth.exr"
    };
    let depth_path = case_dir.join(depth_name);
    write_exr(&color_path, width, height, color)?;
    write_exr(&depth_path, width, height, &depth_pixels(depth))?;

    let mut entries = vec![
        ("name", Json::str(name)),
        ("color", Json::str(color_path.to_string_lossy())),
        ("depth", Json::str(depth_path.to_string_lossy())),
        ("mode", Json::str(mode.name())),
        ("coc_math", Json::Int(mode.coc_math())),
        ("dimensions", Json::ints(&[width as i64, height as i64])),
    ];
    for (key, value) in extra {
        match entries.iter_mut().find(|(existing, _)| *existing == key) {
            Some(entry) => entry.1 = value,
            None => entries.push((key, value)),
        }
    }
    Ok(Json::Object(entries))
}

/// Writes `pixels` as an EXR file at `path`. Fails when the file cannot be written.
fn write_exr(path: &Path, width: usize, height: usize, pixels: &[Rgba]) -> Result<()> {
    fs::write(path, exr::write(width, height, pixels))
        .with_context(|| format!("writing {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::{Mode, write_case};
    use crate::fixtures::scenes::{exr::Rgba, json::Json};

    #[test]
    fn extra_fields_replace_base_keys_in_place() {
        let dir = tempfile::tempdir().expect("temporary directory");
        let color: Vec<Rgba> = vec![[0.0; 4]; 2];
        let depth = vec![1.0; 2];
        let entry = write_case(
            dir.path(),
            "case",
            (&color, &depth),
            Mode::DirectCoc,
            (2, 1),
            vec![
                ("dimensions", Json::ints(&[7, 7])),
                ("gate", Json::str("g")),
            ],
        )
        .expect("case written");
        let Json::Object(fields) = entry else {
            panic!("case is an object");
        };
        let keys: Vec<&str> = fields.iter().map(|(key, _)| *key).collect();
        assert_eq!(
            keys,
            [
                "name",
                "color",
                "depth",
                "mode",
                "coc_math",
                "dimensions",
                "gate"
            ]
        );
        assert!(dir.path().join("case/signed-coc.exr").exists());
    }
}
