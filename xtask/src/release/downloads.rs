//! Generates the downloads page from the OpenFX installers attached to the GitHub releases.
use crate::consts::REPOSITORY;
use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::util::crate_root;

/// Prefix of the installer archives attached to a release.
const INSTALLER_PREFIX: &str = "OpenDefocus-OFX-";

/// Suffix of the installer archives; the `.sha256` files next to them do not match it.
const INSTALLER_SUFFIX: &str = ".tar.gz";

/// Tag prefix of OpenDefocus releases; the other crates are released under their own prefix.
const RELEASE_TAG_PREFIX: &str = "v";

/// Number of releases requested per GitHub API page.
const RELEASES_PER_PAGE: u32 = 100;

/// Builds the HTTP client for the GitHub REST API. It sends the API version header and,
/// when `GITHUB_TOKEN` is set, a bearer token marked as sensitive. Fails when the token
/// is not a valid header value.
fn github_client() -> Result<reqwest::Client> {
    let mut headers = reqwest::header::HeaderMap::new();
    headers.insert(
        reqwest::header::ACCEPT,
        reqwest::header::HeaderValue::from_static("application/vnd.github+json"),
    );
    headers.insert(
        "x-github-api-version",
        reqwest::header::HeaderValue::from_static("2022-11-28"),
    );
    if let Ok(token) = std::env::var("GITHUB_TOKEN") {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))?;
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    Ok(reqwest::Client::builder()
        .user_agent("OpenDefocus xtask")
        .default_headers(headers)
        .build()?)
}

/// Subset of a GitHub release response used by the downloads page.
#[derive(Debug, Deserialize)]
struct GithubRelease {
    /// Git tag of the release, such as `v0.2.0`.
    tag_name: String,
    /// Release notes as written on GitHub, if any.
    body: Option<String>,
    /// Publication time; `None` for drafts.
    published_at: Option<DateTime<Utc>>,
    /// Files attached to the release.
    assets: Vec<GithubAsset>,
}

/// A file attached to a GitHub release.
#[derive(Debug, Deserialize)]
struct GithubAsset {
    /// File name of the asset.
    name: String,
    /// Public URL the asset can be downloaded from.
    browser_download_url: String,
}

/// An OpenDefocus release with its installers, ready to be rendered.
#[derive(Debug, PartialEq)]
struct Release {
    /// Plugin version without the `v` prefix.
    version: String,
    /// Publication date.
    date: DateTime<Utc>,
    /// Release notes.
    changelog: String,
    /// Installer file name and download URL per architecture suffix, sorted by file name.
    installers: Vec<(String, String)>,
}

impl Release {
    /// Builds the release from its GitHub data. Returns `None` when it is not an OpenDefocus
    /// release (other tag prefix), is a draft, or has no OpenFX installer attached.
    fn from_github(release: GithubRelease) -> Option<Self> {
        let version = release.tag_name.strip_prefix(RELEASE_TAG_PREFIX)?;
        if !version.starts_with(|c: char| c.is_ascii_digit()) {
            return None;
        }
        let mut installers: Vec<(String, String)> = release
            .assets
            .into_iter()
            .filter(|asset| {
                asset.name.starts_with(INSTALLER_PREFIX) && asset.name.ends_with(INSTALLER_SUFFIX)
            })
            .map(|asset| (asset.name, asset.browser_download_url))
            .collect();
        if installers.is_empty() {
            return None;
        }
        installers.sort();
        Some(Self {
            version: version.to_owned(),
            date: release.published_at?,
            changelog: release.body.unwrap_or_default(),
            installers,
        })
    }
}

/// Renders the downloads page as Markdown: a table with one row per release, followed by
/// the changelog of each release.
fn releases_to_markdown(releases: &[Release]) -> String {
    let rows: String = releases.iter().map(table_row).collect();
    let changelogs: String = releases.iter().map(changelog_section).collect();
    format!(
        "| Version | Date | Download | Changelog | Source |\n| - | - | - | - | - |\n{rows}\n{changelogs}"
    )
}

/// Renders the changelog section of one release, ending in a newline.
fn changelog_section(release: &Release) -> String {
    format!("## v{}\n{}\n", release.version, release.changelog)
}

/// Renders one table row of the downloads page, ending in a newline.
fn table_row(release: &Release) -> String {
    let downloads = release
        .installers
        .iter()
        .map(|(name, url)| format!("[{name}]({url})"))
        .collect::<Vec<_>>()
        .join("<br>");
    format!(
        "| v{version} | {date} | {downloads} | [Changelog](#v{anchor}) | [Source](https://github.com/{REPOSITORY}/tree/v{version}) |\n",
        version = release.version,
        date = release.date.format("%d-%m-%Y"),
        anchor = release.version.replace('.', ""),
    )
}

/// Lists the published OpenDefocus releases that carry OpenFX installers, newest first.
/// Walks every page of results and fails on any API error.
async fn get_releases() -> Result<Vec<Release>> {
    let client = github_client()?;
    let mut releases = Vec::new();
    for page in 1.. {
        let github_releases: Vec<GithubRelease> = client
            .get(format!(
                "https://api.github.com/repos/{REPOSITORY}/releases?per_page={RELEASES_PER_PAGE}&page={page}"
            ))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        if github_releases.is_empty() {
            break;
        }
        releases.extend(github_releases.into_iter().filter_map(Release::from_github));
    }
    Ok(releases)
}

/// Writes `docs/src/downloads.md` from the published releases.
pub(super) async fn create_downloads_table() -> Result<()> {
    log::info!("Creating downloads markdown...");
    let markdown = releases_to_markdown(&get_releases().await?);
    tokio::fs::write(
        crate_root().join("docs").join("src").join("downloads.md"),
        markdown,
    )
    .await?;
    log::info!("Downloads markdown written");
    Ok(())
}

/// Tests for the downloads page.
#[cfg(test)]
mod tests {
    use super::*;

    /// Parses a GitHub release response for a tag with the given asset names.
    fn github_release(tag: &str, assets: &[&str]) -> GithubRelease {
        serde_json::from_value(serde_json::json!({
            "tag_name": tag,
            "body": "Release notes",
            "published_at": "2026-10-24T09:00:00Z",
            "assets": assets.iter().map(|name| serde_json::json!({
                "name": name,
                "browser_download_url": format!("https://example.test/{name}"),
            })).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    /// Verifies that a release with installers renders into the table and the changelog.
    #[test]
    fn release_with_installers_is_rendered() {
        let release = Release::from_github(github_release(
            "v0.2.0",
            &[
                "OpenDefocus-OFX-0.2.0-linux-x86_64.tar.gz",
                "OpenDefocus-OFX-0.2.0-linux-x86_64.tar.gz.sha256",
                "OpenDefocus-OFX-0.2.0-linux-aarch64.tar.gz",
            ],
        ))
        .unwrap();
        let markdown = releases_to_markdown(&[release]);
        assert!(markdown.contains("| v0.2.0 | 24-10-2026 |"));
        assert!(markdown.contains("[OpenDefocus-OFX-0.2.0-linux-aarch64.tar.gz](https://example.test/OpenDefocus-OFX-0.2.0-linux-aarch64.tar.gz)"));
        assert!(!markdown.contains(".sha256"));
        assert!(
            markdown.contains("[Source](https://github.com/opendefocus/opendefocus/tree/v0.2.0)")
        );
        assert!(markdown.contains("## v0.2.0\nRelease notes"));
    }

    /// Verifies that releases of other crates and releases without installers are skipped.
    #[test]
    fn other_releases_are_skipped() {
        assert!(Release::from_github(github_release("bokeh-creator-v0.2.0", &[])).is_none());
        assert!(
            Release::from_github(github_release("v0.1.10", &["opendefocus-nuke-0.1.10.zip"]))
                .is_none()
        );
    }
}
