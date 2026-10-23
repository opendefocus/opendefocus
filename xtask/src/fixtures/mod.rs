//! Generates, verifies and gates the test fixtures behind `cargo xtask fixtures`. Each
//! subcommand lives in its own submodule.
mod adaptive;
mod quality;
mod render3d;
mod scenes;
mod sync;

use anyhow::Result;

/// Subcommands of `cargo xtask fixtures`.
#[derive(clap::Subcommand, Debug)]
pub enum Fixtures {
    /// Generate or verify the deterministic analytic test scenes in `test/images/scenes`
    Scenes(scenes::Args),
    /// Generate float EXR adaptive-render fixtures outside the checkout
    Adaptive(adaptive::Args),
    /// Run the analytic GPU quality gates and motion sequences on float EXRs
    Quality(quality::Args),
    /// Render the ray-traced 1080p scene (colour and CoC EXRs) for comparing quality modes
    Render3d(render3d::Args),
    /// Stage or verify the packaged copies of the canonical bokeh and circle-of-confusion fixtures
    Sync(sync::Args),
}

/// Runs the selected fixture subcommand and returns its result unchanged.
pub fn run(fixtures: Fixtures) -> Result<()> {
    match fixtures {
        Fixtures::Scenes(args) => scenes::run(args),
        Fixtures::Adaptive(args) => adaptive::run(args),
        Fixtures::Quality(args) => quality::run(args),
        Fixtures::Render3d(args) => render3d::run(args),
        Fixtures::Sync(args) => sync::run(args),
    }
}
