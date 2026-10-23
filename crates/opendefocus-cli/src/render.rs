//! Frame rendering for the CLI, including the striped path that renders padded bands
//! and copies back only each band's own rows, and the cancellation check.

use anyhow::{Result, ensure};
use ndarray::{Array2, Array3, s};
use opendefocus::{
    OpenDefocusRenderer, abort,
    datamodel::{IVector4, Settings, render::RenderSpecs},
};
use std::time::{Duration, Instant};

/// Returns an error once the user has requested cancellation, so no output is saved.
///
/// # Errors
///
/// Returns an error with the message "Render cancelled; output was not saved" when an abort
/// has been requested.
pub fn check_cancelled() -> Result<()> {
    ensure!(
        !abort::get_aborted(),
        "Render cancelled; output was not saved"
    );
    Ok(())
}

/// Renders one frame, optionally in horizontal stripes, and returns the result with timings.
///
/// With `stripe_height`, each stripe is rendered from a padded band of the original source
/// (and depth and filter) and only its own rows are copied to the output. Without it, the
/// whole frame is rendered in one call. The returned tuple is the output, the total wall
/// time, and the time spent inside renderer calls.
///
/// # Errors
///
/// Returns an error if cancellation is requested or any renderer call fails.
pub async fn render_frame(
    renderer: &OpenDefocusRenderer,
    settings: &Settings,
    source: &Array3<f32>,
    depth: Option<&Array2<f32>>,
    filter: Option<&Array3<f32>>,
    stripe_height: Option<u32>,
) -> Result<(Array3<f32>, Duration, Duration)> {
    let start = Instant::now();
    let mut output = source.clone();
    let mut calls = Duration::ZERO;
    if let Some(stripe_height) = stripe_height {
        let (height, width, _) = source.dim();
        let padding = renderer.padding(settings) as usize;
        for y in (0..height).step_by(stripe_height as usize) {
            check_cancelled()?;
            let end = y.saturating_add(stripe_height as usize).min(height);
            let top = y.saturating_sub(padding);
            let bottom = end.saturating_add(padding).min(height);
            // Every padded band reads the original source, including depth.
            let mut band = source.slice(s![top..bottom, .., ..]).to_owned();
            let depth = depth.map(|d| d.slice(s![top..bottom, ..]).to_owned());
            let filter = filter.cloned();
            let specs = RenderSpecs {
                full_region: IVector4 {
                    x: 0,
                    y: top as i32,
                    z: width as i32,
                    w: bottom as i32,
                },
                render_region: IVector4 {
                    x: 0,
                    y: y as i32,
                    z: width as i32,
                    w: end as i32,
                },
            };
            let call = Instant::now();
            renderer
                .render_stripe(specs, settings.clone(), band.view_mut(), depth, filter)
                .await?;
            calls += call.elapsed();
            output.slice_mut(s![y..end, .., ..]).assign(&band.slice(s![
                y - top..end - top,
                ..,
                ..
            ]));
        }
    } else {
        let depth = depth.cloned();
        let filter = filter.cloned();
        let call = Instant::now();
        renderer
            .render(settings.clone(), output.view_mut(), depth, filter)
            .await?;
        calls = call.elapsed();
    }
    check_cancelled()?;
    Ok((output, start.elapsed(), calls))
}
