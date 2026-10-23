//! Unit tests for the render geometry: rectangle sizes and containment, source padding, frame
//! coordinates, canonical-to-pixel mapping, render-scale settings and the lens centre.

use super::*;

/// Checks rectangle width, height and containment, including edges that lie outside.
#[test]
fn rect_contains_and_sizes() {
    let window = Rect::from_ints([0, 0, 64, 32]);
    assert_eq!(window.width(), 64);
    assert_eq!(window.height(), 32);
    assert!(window.contains(Rect::from_ints([0, 0, 64, 32])));
    assert!(window.contains(Rect::from_ints([10, 2, 20, 9])));
    assert!(!window.contains(Rect::from_ints([0, 0, 65, 32])));
    assert!(!window.contains(Rect::from_ints([-1, 0, 64, 32])));
}

/// Checks that the source request is the window padded and clamped to the region of definition.
#[test]
fn source_request_pads_and_clamps_to_the_region_of_definition() {
    let rod = Rect::from_ints([0, 0, 96, 80]);
    let window = Rect::from_ints([40, 30, 96, 80]);
    assert_eq!(
        source_request(window, rod, 8, false),
        Rect::from_ints([32, 22, 96, 80])
    );
    // Zero padding asks for the window itself.
    assert_eq!(source_request(window, rod, 0, false), window);
    // A padding larger than the frame clamps to the whole region.
    assert_eq!(source_request(window, rod, 500, false), rod);
}

/// Checks that a remote-reading render requests the whole region of definition, not the padded window.
#[test]
fn remote_source_requests_the_whole_region_of_definition() {
    let rod = Rect::from_ints([0, 0, 96, 80]);
    let window = Rect::from_ints([40, 30, 60, 50]);
    assert_eq!(source_request(window, rod, 4, true), rod);
}

/// Checks that OFX bottom-up rectangles map to top-down frame coordinates relative to the region.
#[test]
fn top_down_frame_coordinates_follow_the_region_of_definition() {
    let rod = Rect::from_ints([10, 20, 106, 100]);
    let full = top_down(rod, rod);
    assert_eq!((full.x, full.y, full.z, full.w), (0, 0, 96, 80));
    // The bottom-left OFX pixel block is the bottom-left block of the frame.
    let corner = top_down(Rect::from_ints([10, 20, 14, 24]), rod);
    assert_eq!((corner.x, corner.y, corner.z, corner.w), (0, 76, 4, 80));
}

/// Checks canonical-to-pixel conversion at reduced scale, pixel aspect ratio and outward rounding.
#[test]
fn pixel_map_converts_canonical_rectangles_at_reduced_scale() {
    let half = PixelMap {
        scale_x: 0.5,
        scale_y: 0.5,
        par: 1.0,
    };
    assert_eq!(
        half.to_pixels([0.0, 0.0, 1920.0, 1080.0]),
        Rect::from_ints([0, 0, 960, 540])
    );
    assert_eq!(
        half.to_canonical(Rect::from_ints([0, 0, 960, 540])),
        [0.0, 0.0, 1920.0, 1080.0]
    );
    // Pixels round outwards: [0.5, 2.5] covers pixels 0, 1 and 2.
    assert_eq!(
        half.to_pixels([1.0, 1.0, 5.0, 5.0]),
        Rect::from_ints([0, 0, 3, 3])
    );
    let quarter = PixelMap {
        scale_x: 0.25,
        scale_y: 0.25,
        par: 1.0,
    };
    assert_eq!(
        quarter.to_pixels([0.0, 0.0, 96.0, 80.0]),
        Rect::from_ints([0, 0, 24, 20])
    );
    // Full scale with square pixels keeps the values.
    let full = PixelMap {
        scale_x: 1.0,
        scale_y: 1.0,
        par: 1.0,
    };
    assert_eq!(
        full.to_pixels([10.0, 20.0, 30.0, 40.0]),
        Rect::from_ints([10, 20, 30, 40])
    );
    assert_eq!(
        full.to_canonical(Rect::from_ints([10, 20, 30, 40])),
        [10.0, 20.0, 30.0, 40.0]
    );
    // A pixel aspect ratio of 2 makes each pixel two canonical units wide.
    let squeezed = PixelMap {
        scale_x: 1.0,
        scale_y: 1.0,
        par: 2.0,
    };
    assert_eq!(
        squeezed.to_pixels([0.0, 0.0, 100.0, 50.0]),
        Rect::from_ints([0, 0, 50, 50])
    );
    assert_eq!(
        squeezed.to_canonical(Rect::from_ints([0, 0, 50, 50])),
        [0.0, 0.0, 100.0, 50.0]
    );
}

/// Checks that a reduced render scale shrinks only the pixel-measured circle of confusion sizes.
#[test]
fn scale_pixel_settings_scales_only_the_pixel_sizes() {
    let mut settings = Settings::default();
    let before = settings.clone();
    scale_pixel_settings(&mut settings, 1.0);
    assert_eq!(settings, before);
    settings.defocus.circle_of_confusion.size = 8.0;
    settings.defocus.circle_of_confusion.max_size = 12.0;
    settings.defocus.camera_max_size = 20.0;
    scale_pixel_settings(&mut settings, 0.5);
    assert_eq!(settings.defocus.circle_of_confusion.size, 4.0);
    assert_eq!(settings.defocus.circle_of_confusion.max_size, 6.0);
    assert_eq!(settings.defocus.camera_max_size, 10.0);
    assert_eq!(settings.bokeh, before.bokeh);
    assert_eq!(settings.non_uniform, before.non_uniform);
}

/// Checks that the lens centre is the middle of the full frame, not of the tile.
#[test]
fn frame_centre_is_the_middle_of_the_full_frame() {
    // Regression: the centre stayed at (0, 0), so catseye and the other lens effects were
    // centred on the bottom-left corner.
    let mut settings = Settings::default();
    set_frame(&mut settings, Rect::from_ints([100, 50, 2020, 1130]));
    assert_eq!(
        (settings.render.resolution.x, settings.render.resolution.y),
        (1920, 1080)
    );
    assert_eq!(
        (settings.render.center.x, settings.render.center.y),
        (960.0, 540.0)
    );
}

/// Checks that an inverted rectangle reports zero width and height.
#[test]
fn empty_rect_has_no_size() {
    assert_eq!(Rect::from_ints([5, 5, 3, 3]).width(), 0);
    assert_eq!(Rect::from_ints([5, 5, 3, 3]).height(), 0);
}
