//! Asset contracts, independent of the renderer's visual output.
use std::path::PathBuf;

#[test]
#[allow(
    clippy::float_cmp,
    reason = "the generated aperture mask stores exact 0.0 and 1.0 values"
)]
/// Verifies the HDR, depth and alpha contracts of the diagnostic assets.
fn test_diagnostic_hdr_depth_and_alpha_contracts() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../test/images/scenes");
    for name in ["occlusion", "focus_ramp", "hdr_points", "alpha_edges"] {
        let load = |file| {
            image::open(root.join(name).join(file))
                .unwrap()
                .to_rgba32f()
        };
        let color = load("color.exr");
        let depth = load("depth.exr");
        let coc = load("coc.exr");
        assert_eq!(color.dimensions(), (192, 128));
        assert_eq!(depth.dimensions(), color.dimensions());
        assert_eq!(coc.dimensions(), color.dimensions());
        let mut fractional_alpha = false;
        let mut hdr = false;
        for ((rgba, z), radius) in color.pixels().zip(depth.pixels()).zip(coc.pixels()) {
            assert!(
                rgba.0
                    .iter()
                    .chain(z.0.iter())
                    .chain(radius.0.iter())
                    .all(|v| v.is_finite())
            );
            assert!((0.0..=1.0).contains(&rgba[3]));
            assert!(z[0] > 0.0);
            assert!((radius[0] - 24.0 * (1.0 - 4.0 / z[0])).abs() < 5e-6);
            assert!(radius[0].abs() <= 24.0);
            assert_eq!(z[0], z[1]);
            assert_eq!(z[0], z[2]);
            assert_eq!(radius[0], radius[1]);
            assert_eq!(radius[0], radius[2]);
            if rgba[3] == 0.0 {
                assert_eq!(&rgba.0[..3], &[0.0; 3]);
            }
            fractional_alpha |= rgba[3] > 0.0 && rgba[3] < 1.0;
            hdr |= rgba.0[..3].iter().any(|v| *v > 1.0);
        }
        if name == "alpha_edges" {
            assert!(fractional_alpha);
        }
        if name == "hdr_points" {
            assert!(hdr);
        }
    }
    let aperture = image::open(root.join("hdr_points/aperture.exr"))
        .unwrap()
        .to_rgba32f();
    assert_eq!(aperture.dimensions(), (33, 33));
    assert!(aperture.pixels().any(|p| p[0] == 1.0));
    assert!(aperture.pixels().any(|p| p[0] == 0.0));
    // Its deliberate off-center notch distinguishes orientation from a symmetric disk.
    assert_ne!(aperture.get_pixel(22, 8)[0], aperture.get_pixel(10, 24)[0]);
}
