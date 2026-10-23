//! State tests for the viewer: default evaluation settings, dirty and render gating,
//! worker send failures, and which focus or filter actions are available.

use super::*;

/// Builds a viewer whose worker channels stay open; the job receiver is returned so tests can read sent jobs.
fn viewer() -> (Viewer, Receiver<Job>) {
    let (jobs, input) = mpsc::channel();
    let (_output, results) = mpsc::channel();
    (Viewer::from_channels(jobs, results), input)
}

/// Checks the startup settings: 2D mode, reciprocal depth, disc bokeh and accurate quality.
#[test]
fn evaluation_settings_start_in_depth_mode_with_disc_accurate_quality() {
    let settings = evaluation_settings();
    assert_eq!(settings.defocus.defocus_mode(), DefocusMode::Twod);
    assert!(!settings.defocus.use_direct_math);
    assert_eq!(settings.defocus.circle_of_confusion.math, 1);
    assert_eq!(settings.bokeh.filter_type(), FilterType::Disc);
    assert_eq!(settings.render.quality(), Quality::Accurate);
}

/// Checks that a new viewer has no image and reports the startup status line.
#[test]
fn new_viewer_is_idle_without_image() {
    let (viewer, _input) = viewer();
    assert_eq!(viewer.status, "Load an image to begin");
    assert!(viewer.original.is_none());
    assert!(!viewer.busy);
    assert!(!viewer.dirty);
    assert!(viewer.auto_render);
    assert!(!viewer.can_render());
}

/// Checks that a settings change marks the result dirty and stamps the change time.
#[test]
fn mark_dirty_sets_dirty_and_moves_change_time_forward() {
    let (mut viewer, _input) = viewer();
    let before = viewer.changed_at;
    std::thread::sleep(Duration::from_millis(2));
    viewer.mark_dirty();
    assert!(viewer.dirty);
    assert!(viewer.changed_at > before);
}

/// Checks that rendering is skipped without a visible region or filter preview, and keeps the dirty flag.
#[test]
fn render_without_region_or_filter_preview_sends_nothing() {
    let (mut viewer, input) = viewer();
    viewer.dirty = true;
    viewer.render();
    assert!(viewer.dirty);
    assert!(!viewer.busy);
    assert!(matches!(input.try_recv(), Err(TryRecvError::Empty)));
}

/// Checks that a render of the visible region sends it with the current settings and clears the dirty flags.
#[test]
fn render_sends_visible_region_and_clears_dirty_flags() {
    let (mut viewer, input) = viewer();
    let region = IVector4 {
        x: 0,
        y: 0,
        z: 4,
        w: 3,
    };
    viewer.visible_region = Some(region);
    viewer.dirty = true;
    viewer.viewport_dirty = true;
    viewer.render();
    assert!(!viewer.dirty);
    assert!(!viewer.viewport_dirty);
    assert!(viewer.busy);
    match input.try_recv() {
        Ok(Job::Render {
            region: Some(sent),
            settings,
        }) => {
            assert_eq!(sent, region);
            assert_eq!(settings, viewer.settings);
        }
        _ => panic!("expected a render job for the visible region"),
    }
}

/// Checks that a filter preview renders even without a visible region.
#[test]
fn filter_preview_renders_without_visible_region() {
    let (mut viewer, input) = viewer();
    viewer
        .settings
        .render
        .filter
        .set_mode(FilterMode::BokehCreator);
    viewer.settings.render.filter.preview = true;
    assert!(viewer.filter_preview_only());
    assert!(viewer.can_render());
    viewer.render();
    assert!(matches!(
        input.try_recv(),
        Ok(Job::Render { region: None, .. })
    ));
}

/// Checks that a render after the worker has stopped records an error and does not stay busy.
#[test]
fn send_after_worker_stopped_records_error() {
    let (mut viewer, input) = viewer();
    drop(input);
    viewer.visible_region = Some(IVector4::default());
    viewer.render();
    assert!(!viewer.busy);
    assert_eq!(
        viewer.error.as_deref(),
        Some("Rendering worker stopped; restart the application")
    );
}

/// Checks that focus picking needs a depth map, depth mode, reciprocal math and no filter preview.
#[test]
fn focus_picking_requires_depth_map_and_no_direct_math_or_preview() {
    let (mut viewer, _input) = viewer();
    viewer.settings.defocus.set_defocus_mode(DefocusMode::Depth);
    assert!(!viewer.can_pick_focus());
    viewer.depth_name = Some("depth.exr".into());
    assert!(viewer.can_pick_focus());
    viewer.settings.defocus.use_direct_math = true;
    assert!(!viewer.can_pick_focus());
    viewer.settings.defocus.use_direct_math = false;
    viewer.settings.render.filter.preview = true;
    assert!(!viewer.can_pick_focus());
}

/// Checks that the filter preview is the only render target only in bokeh-creator mode with preview on.
#[test]
fn filter_preview_only_needs_preview_and_bokeh_creator_mode() {
    let (mut viewer, _input) = viewer();
    assert!(!viewer.filter_preview_only());
    viewer.settings.render.filter.preview = true;
    assert!(!viewer.filter_preview_only());
    viewer
        .settings
        .render
        .filter
        .set_mode(FilterMode::BokehCreator);
    assert!(viewer.filter_preview_only());
}
