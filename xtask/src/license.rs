//! Generates the third-party license notice: `cargo about` renders the dependencies to
//! HTML, which is then converted to Markdown.
use std::path::PathBuf;

use crate::util::crate_root;
use anyhow::{Context, Result};
use duct::cmd;
use html_to_markdown_rs::convert;
/// Writes the dependency license notice to `target_file` as Markdown. Does nothing when
/// the `SKIP_LICENSE` variable was set at compile time. Fails when `cargo about` fails,
/// when the file cannot be read or written, or when the conversion yields no Markdown.
pub async fn fetch_licenses(target_file: PathBuf) -> Result<()> {
    if option_env!("SKIP_LICENSE").is_some() {
        return Ok(());
    }
    log::info!("Fetching dependencies license information...");
    let about_config = format!("{}/../licenses.hbs", env!("CARGO_MANIFEST_DIR"));
    cmd!(
        "cargo",
        "about",
        "generate",
        "--all-features",
        "-o",
        &target_file,
        about_config
    )
    .dir(crate_root())
    .run()?;

    let contents = tokio::fs::read_to_string(&target_file).await?;
    let markdown = convert(&contents, None)?
        .content
        .context("License conversion produced no Markdown")?;
    tokio::fs::write(target_file, markdown).await?;
    log::info!("License information written");
    Ok(())
}

/// Tests for the HTML-to-Markdown conversion of the license notice.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that HTML headings and emphasis are converted to the expected Markdown.
    #[test]
    fn test_license_markdown_without_metadata_feature() {
        let markdown = convert(
            "<h1>Licenses</h1><p>Example <strong>license</strong>.</p>",
            None,
        )
        .unwrap()
        .content
        .unwrap();
        assert!(markdown.contains("Licenses"));
        assert!(markdown.contains("Example **license**."));
    }
}
