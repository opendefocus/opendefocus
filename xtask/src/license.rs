
use std::path::PathBuf;

use anyhow::Result;
use duct::cmd;
use html_to_markdown_rs::{ConversionOptions, converter::convert_html};
pub async fn fetch_licenses(target_file: PathBuf) -> Result<()> {
    let about_config = format!("{}/../licenses.hbs", env!("CARGO_MANIFEST_DIR"));
    cmd!("cargo", "about", "generate", "--all-features", "-o", &target_file, about_config).run()?;

    let contents = tokio::fs::read_to_string(&target_file).await?;
    let markdown = convert_html(&contents, &ConversionOptions::default())?;
    tokio::fs::write(target_file, markdown).await?;
    Ok(())
}