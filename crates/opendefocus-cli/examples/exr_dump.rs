//! Dump an EXR's FLOAT RGBA channels as little-endian raw files for QA tools.
//!
//! Run with `cargo run --release -p opendefocus-cli --example exr_dump -- \
//! input.exr /tmp/output-prefix`. The program prints `WIDTH HEIGHT` and writes
//! `PREFIX-r.f32`, `PREFIX-g.f32`, `PREFIX-b.f32`, and `PREFIX-a.f32`.

use std::{
    env,
    fs::File,
    io::{BufWriter, Write},
    path::PathBuf,
};

use anyhow::{Context, bail};

/// Writes the four float channels of the input EXR as raw little-endian files and prints its size.
///
/// # Errors
///
/// Returns an error if the arguments are wrong, the image cannot be loaded, or an output file cannot be written.
fn main() -> anyhow::Result<()> {
    let mut args = env::args_os().skip(1);
    let input = args
        .next()
        .map(PathBuf::from)
        .context("missing input EXR path")?;
    let prefix = args
        .next()
        .map(PathBuf::from)
        .context("missing output prefix")?;
    if args.next().is_some() {
        bail!("usage: exr_dump INPUT.exr OUTPUT_PREFIX");
    }

    let rgba = image::open(&input)
        .with_context(|| format!("loading {}", input.display()))?
        .to_rgba32f();
    let (width, height) = rgba.dimensions();
    println!("{width} {height}");

    let channels = ["r", "g", "b", "a"];
    for (index, name) in channels.iter().enumerate() {
        let path = PathBuf::from(format!("{}-{name}.f32", prefix.display()));
        let mut output = BufWriter::new(
            File::create(&path).with_context(|| format!("creating {}", path.display()))?,
        );
        for pixel in rgba.pixels() {
            output.write_all(&pixel[index].to_le_bytes())?;
        }
        output.flush()?;
    }
    Ok(())
}
