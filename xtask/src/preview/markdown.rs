//! Formats the pull request comment from the rendered workloads.
use super::{compare::Difference, render::Render, sweep};
/// Width in pixels of each image in the comment table.
const IMAGE_WIDTH: u32 = 220;
/// Relative slowdown, in percent, from which a row is flagged.
const SLOWDOWN_FLAG_PERCENT: f64 = 5.0;

/// The results for one workload.
pub struct Row {
    /// Workload name.
    pub name: String,
    /// Render of the pull request build.
    pub head: Render,
    /// Render of the base build, when one was given.
    pub base: Option<Render>,
    /// Pixel difference between base and head, when both exist.
    pub difference: Option<Difference>,
}

/// Returns an HTML image tag for `path` below `url`.
fn image(url: &str, path: &str) -> String {
    format!("<img src=\"{url}/{path}\" width=\"{IMAGE_WIDTH}\">")
}

/// Describes the timing of a row: `base → head ms (±n%)`, or only the head time.
fn timing(row: &Row) -> String {
    let head = row.head.median_ms;
    let Some(base) = row.base.as_ref().map(|base| base.median_ms) else {
        return format!("{head:.1} ms");
    };
    let change = (head / base - 1.0) * 100.0;
    let flag = if change >= SLOWDOWN_FLAG_PERCENT {
        " ⚠️"
    } else {
        ""
    };
    format!("{base:.1} → {head:.1} ms ({change:+.1}%){flag}")
}

/// Describes the pixel difference of a row.
fn pixels(row: &Row) -> String {
    match &row.difference {
        None => "–".to_owned(),
        Some(difference) if difference.max_abs == 0.0 => "identical".to_owned(),
        Some(difference) => format!("max {:.2e}, rms {:.2e}", difference.max_abs, difference.rms),
    }
}

/// Formats one table line of the comment for `row`, with image paths below `url`.
fn table_row(row: &Row, url: &str) -> String {
    let name = &row.name;
    let optional = |present: bool, path: String| {
        if present {
            image(url, &path)
        } else {
            "–".to_owned()
        }
    };
    let base = optional(row.base.is_some(), format!("base/{name}.png"));
    let diff = optional(row.difference.is_some(), format!("diff/{name}.png"));
    let head = image(url, &format!("head/{name}.png"));
    format!(
        "| `{name}` | {} | {} | {base} | {head} | {diff} |\n",
        timing(row),
        pixels(row)
    )
}

/// Width in pixels of each animation in the comment.
const ANIMATION_WIDTH: u32 = 360;

/// Formats the focal sweep section: the animations of the available builds side by side and
/// the largest difference between base and head.
fn sweep_section(sweep: &sweep::Sweep, url: &str) -> String {
    let animation = |path: &str| format!("<img src=\"{url}/{path}\" width=\"{ANIMATION_WIDTH}\">");
    let intro = format!(
        "\n### Focal sweep\n\nThe 3D scene at {} focal planes from near to far and back, Fast \
         quality, lossless frames. Watch for flicker, popping and banding while the focus moves.\n\n",
        sweep.focal_steps
    );
    let body = match sweep.max_difference {
        Some(max) => {
            let verdict = if max == 0.0 {
                "identical".to_owned()
            } else {
                format!("max {max:.2e}")
            };
            format!(
                "Base against head: {verdict}.\n\n| Base | Head | Difference ×16 |\n| --- | --- | --- |\n| {} | {} | {} |\n",
                animation(sweep::BASE_ANIMATION),
                animation(sweep::HEAD_ANIMATION),
                animation(sweep::DIFFERENCE_ANIMATION),
            )
        }
        None => format!("{}\n", animation(sweep::HEAD_ANIMATION)),
    };
    intro + &body
}

/// Builds the Markdown comment. Image paths are `head/<name>.png`, `base/<name>.png` and
/// `diff/<name>.png` below `url`; the focal sweep animations are added when `sweep` is given and
/// a link to the web viewer when `web_url` is.
pub fn comment(
    rows: &[Row],
    sweep: Option<&sweep::Sweep>,
    url: &str,
    web_url: Option<&str>,
) -> String {
    let url = url.trim_end_matches('/');
    let device = rows
        .first()
        .map_or("unknown", |row| row.head.device.as_str());
    let web = web_url.map_or_else(String::new, |link| {
        format!("[Open the web viewer built from this pull request]({link})\n\n")
    });
    let mut text = format!(
        "## Render preview\n\n{web}Rendered on `{device}` on a hosted runner. The hardware is shared and \
         virtual, so timings are noisy and say little about real GPU speed, only about a change \
         between base and head. A ⚠️ marks a slowdown of {SLOWDOWN_FLAG_PERCENT}% or more.\n\n\
         | Scene | Median frame | Pixels | Base | Head | Difference ×16 |\n\
         | --- | --- | --- | --- | --- | --- |\n"
    );
    for row in rows {
        text.push_str(&table_row(row, url));
    }
    if let Some(sweep) = sweep {
        text.push_str(&sweep_section(sweep, url));
    }
    text
}

/// Tests for the comment formatting.
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// Builds a render with the given median.
    fn render(median_ms: f64) -> Render {
        Render {
            exr: PathBuf::new(),
            median_ms,
            device: "llvmpipe".into(),
        }
    }

    /// Verifies the timing text, the slowdown flag and the identical pixel marker.
    #[test]
    fn test_comment_reports_change_and_flags_slowdowns() {
        let rows = [Row {
            name: "scene_full".into(),
            head: render(110.0),
            base: Some(render(100.0)),
            difference: Some(Difference {
                max_abs: 0.0,
                rms: 0.0,
            }),
        }];
        let text = comment(&rows, None, "https://example.test/pr-1/", None);
        assert!(text.contains("100.0 → 110.0 ms (+10.0%) ⚠️"));
        assert!(text.contains("identical"));
        assert!(text.contains("https://example.test/pr-1/head/scene_full.png"));
        assert!(!text.contains("pr-1//"));
    }

    /// Verifies that a row without a base shows only the head time and no flag.
    #[test]
    fn test_comment_without_base_shows_head_only() {
        let rows = [Row {
            name: "a".into(),
            head: render(12.34),
            base: None,
            difference: None,
        }];
        let text = comment(&rows, None, "u", Some("https://web.test/"));
        assert!(text.contains("12.3 ms"));
        assert!(!text.contains("⚠️ |"));
        assert!(text.contains("(https://web.test/)"));
    }
}
