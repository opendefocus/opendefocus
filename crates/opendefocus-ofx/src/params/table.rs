//! The parameter table: one [`ParamDef`] row per plugin parameter, with its host definition and the
//! function that writes its value into the render settings. Row order is the display order.

use super::{
    AxialAberrationType, CAMERA_TOGGLE, CameraData, DIRECT_MATH, FilterMode, Math, ParamKind,
    ParamValue, Quality, Settings, WorldUnit, boolean, camera_mut, choice, float, int,
};

/// One row of the parameter table.
#[derive(Clone, Copy)]
pub struct ParamDef {
    /// Stable host identifier. Never rename after release; saved projects use it.
    pub name: &'static str,
    /// Caption shown next to the control.
    pub label: &'static str,
    /// Tooltip text describing what the parameter does.
    pub hint: &'static str,
    /// Host type and range of the parameter, with its default.
    pub kind: ParamKind,
    /// Name of the group in [`GROUPS`] that contains this parameter.
    pub group: &'static str,
    /// Writes a value into the settings. Receives a value of the matching variant.
    pub apply: fn(&mut Settings, ParamValue),
}

impl ParamDef {
    /// The default value of this parameter as a [`ParamValue`].
    #[must_use]
    pub fn default_value(&self) -> ParamValue {
        match self.kind {
            ParamKind::Double { default, .. } => ParamValue::Double(default),
            ParamKind::Int { default, .. } => ParamValue::Int(default),
            ParamKind::Bool { default } => ParamValue::Bool(default),
            ParamKind::Choice { default, .. } => ParamValue::Choice(default),
        }
    }
}

/// Parameters exposed by the plugin, in display order.
///
/// Defaults must equal the matching fields of `Settings::default()`.
pub const PARAMS: &[ParamDef] = &[
    ParamDef {
        name: "defocusMode",
        label: "Defocus Mode",
        hint: "2D convolution or convolution by depth",
        kind: ParamKind::Choice {
            options: &["2D", "Depth"],
            default: 0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.defocus_mode = choice(v),
    },
    ParamDef {
        name: "size",
        label: "Size",
        hint: "Base circle of confusion size",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 5.0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.circle_of_confusion.size = float(v),
    },
    ParamDef {
        name: "maxSize",
        label: "Max Size",
        hint: "Maximum circle of confusion size",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 10.0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.circle_of_confusion.max_size = float(v),
    },
    ParamDef {
        name: "focalPlane",
        label: "Focal Plane",
        hint: "Depth value that stays in focus. A positive focus distance is required when the camera is used (Use Camera or a connected Camera input)",
        kind: ParamKind::Double {
            min: -1000000.0,
            max: 1000000.0,
            default: 0.0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.circle_of_confusion.focal_plane = float(v),
    },
    ParamDef {
        name: "quality",
        label: "Quality",
        hint: "Fast: quick preview-grade approximation of Accurate. Accurate: exact, slower",
        kind: ParamKind::Choice {
            options: &["Fast", "Accurate"],
            default: Quality::Accurate as i32,
        },
        group: "defocus",
        // The CLI renders with farm quality (gui = false) and sets both fields when given
        // --quality. The renderer reads `quality` only when gui is true, so set both.
        apply: |s, v| {
            s.render.quality = choice(v);
            s.render.farm_quality = choice(v);
        },
    },
    ParamDef {
        name: "math",
        label: "Math",
        hint: "How the depth input is read. Real: distance from the camera. 1/Z: Nuke and RenderMan depth. Direct: the value is the defocus size in pixels",
        kind: ParamKind::Choice {
            options: &["Real", "1/Z", "Direct"],
            default: Math::OneDividedByZ as i32,
        },
        group: "defocus",
        apply: |s, v| {
            // Index 2 is Direct, which has no `Math` variant: it is a separate settings flag.
            let index = choice(v);
            s.defocus.use_direct_math = index == DIRECT_MATH;
            if index != DIRECT_MATH {
                s.defocus.circle_of_confusion.math = index;
            }
        },
    },
    ParamDef {
        name: "protect",
        label: "Protect",
        hint: "Width of the in-focus band around the focal plane",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 0.0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.circle_of_confusion.protect = float(v),
    },
    ParamDef {
        name: "sizeMultiplier",
        label: "Size Multiplier",
        hint: "Scales the final defocus size",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10.0,
            default: 1.0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.size_multiplier = float(v),
    },
    ParamDef {
        name: "gamma",
        label: "Gamma",
        hint: "Gamma correction that boosts bokeh highlights",
        kind: ParamKind::Double {
            min: 0.01,
            max: 10.0,
            default: 1.0,
        },
        group: "defocus",
        apply: |s, v| s.defocus.gamma_correction = float(v),
    },
    // Camera group. The toggle must stay the first row: camera rows only write into the
    // camera data the toggle creates, so they must be applied after it.
    ParamDef {
        name: CAMERA_TOGGLE,
        label: "Use Camera",
        hint: "Compute the circle of confusion from the camera values below",
        kind: ParamKind::Bool { default: false },
        group: "camera",
        apply: |s, v| {
            let data = &mut s.defocus.circle_of_confusion.camera_data;
            if boolean(v) {
                data.get_or_insert_with(CameraData::default);
            } else {
                *data = None;
            }
        },
    },
    ParamDef {
        name: "focalLength",
        label: "Focal Length",
        hint: "Lens focal length in millimetres",
        kind: ParamKind::Double {
            min: 1.0,
            max: 10000.0,
            default: 50.0,
        },
        group: "camera",
        apply: |s, v| {
            if let Some(camera) = camera_mut(s) {
                camera.focal_length = float(v);
            }
        },
    },
    ParamDef {
        name: "cameraMaxSize",
        label: "Camera Max Size",
        hint: "Maximum defocus size in pixels when the camera drives the defocus",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10000.0,
            default: 10.0,
        },
        group: "camera",
        apply: |s, v| s.defocus.camera_max_size = float(v),
    },
    ParamDef {
        name: "fStop",
        label: "F-Stop",
        hint: "Lens aperture; lower values give a larger defocus",
        kind: ParamKind::Double {
            min: 0.1,
            max: 128.0,
            default: 16.0,
        },
        group: "camera",
        apply: |s, v| {
            if let Some(camera) = camera_mut(s) {
                camera.f_stop = float(v);
            }
        },
    },
    ParamDef {
        name: "filmbackWidth",
        label: "Filmback Width",
        hint: "Sensor width in millimetres",
        kind: ParamKind::Double {
            min: 0.1,
            max: 1000.0,
            default: 24.576,
        },
        group: "camera",
        apply: |s, v| {
            if let Some(camera) = camera_mut(s) {
                camera.filmback.width = float(v);
            }
        },
    },
    ParamDef {
        name: "filmbackHeight",
        label: "Filmback Height",
        hint: "Sensor height in millimetres",
        kind: ParamKind::Double {
            min: 0.1,
            max: 1000.0,
            default: 18.672,
        },
        group: "camera",
        apply: |s, v| {
            if let Some(camera) = camera_mut(s) {
                camera.filmback.height = float(v);
            }
        },
    },
    ParamDef {
        name: "worldUnit",
        label: "World Unit",
        hint: "Unit of the scene distances, used for the focal plane and camera maths",
        kind: ParamKind::Choice {
            // Indexed by the raw `WorldUnit` value: Mm=0, Cm=1, Dm=2, M=3, In=4, Ft=5.
            options: &["mm", "cm", "dm", "m", "in", "ft"],
            default: WorldUnit::M as i32,
        },
        group: "camera",
        apply: |s, v| {
            if let Some(camera) = camera_mut(s) {
                camera.world_unit = choice(v);
            }
        },
    },
    ParamDef {
        name: "inverseForeground",
        label: "Inverse Foreground",
        hint: "Apply the non-uniform effects inverted for foreground",
        kind: ParamKind::Bool { default: true },
        group: "nonUniform",
        apply: |s, v| s.non_uniform.inverse_foreground = boolean(v),
    },
    ParamDef {
        name: "bokehFilterType",
        label: "Filter Type",
        hint: "Simple uses the fast built-in filter and ignores the other Bokeh settings. Disc and Blade generate the bokeh shape: round or polygon blades",
        kind: ParamKind::Choice {
            // Index 0 is Simple (FilterMode::Simple). Indices 1 and 2 are the bokeh
            // creator's Disc (0) and Blade (1) filter types.
            options: &["Simple", "Disc", "Blade"],
            default: 0,
        },
        group: "bokeh",
        apply: |s, v| {
            let index = choice(v);
            s.render.filter.set_mode(if index == 0 {
                FilterMode::Simple
            } else {
                FilterMode::BokehCreator
            });
            s.bokeh.filter_type = (index - 1).max(0);
        },
    },
    ParamDef {
        name: "bokehRadius",
        label: "Radius",
        hint: "Radius of the bokeh kernel",
        kind: ParamKind::Double {
            min: 0.0,
            max: 100.0,
            default: 1.0,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.radius = float(v),
    },
    ParamDef {
        name: "bokehRingColor",
        label: "Ring Color",
        hint: "Brightness of the bokeh ring",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10.0,
            default: 1.0,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.ring_color = float(v),
    },
    ParamDef {
        name: "bokehInnerColor",
        label: "Inner Color",
        hint: "Brightness of the bokeh interior",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10.0,
            default: 0.4,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.inner_color = float(v),
    },
    ParamDef {
        name: "bokehRingSize",
        label: "Ring Size",
        hint: "Width of the bokeh ring",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1.0,
            default: 0.1,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.ring_size = float(v),
    },
    ParamDef {
        name: "bokehOuterBlur",
        label: "Outer Blur",
        hint: "Softness of the outer bokeh edge",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1.0,
            default: 0.1,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.outer_blur = float(v),
    },
    ParamDef {
        name: "bokehInnerBlur",
        label: "Inner Blur",
        hint: "Softness of the inner bokeh edge",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1.0,
            default: 0.05,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.inner_blur = float(v),
    },
    ParamDef {
        name: "bokehBlades",
        label: "Blades",
        hint: "Number of aperture blades for the Blade filter type",
        kind: ParamKind::Int {
            min: 3,
            max: 64,
            default: 5,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.blades = int(v) as u32,
    },
    ParamDef {
        name: "bokehAngle",
        label: "Angle",
        hint: "Rotation of the bokeh shape",
        kind: ParamKind::Double {
            min: -360.0,
            max: 360.0,
            default: 0.0,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.angle = float(v),
    },
    ParamDef {
        name: "bokehCurvature",
        label: "Curvature",
        hint: "How round the blade edges are",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1.0,
            default: 0.5,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.curvature = float(v),
    },
    ParamDef {
        name: "bokehAspectRatio",
        label: "Aspect Ratio",
        hint: "Squeeze of the bokeh shape",
        kind: ParamKind::Double {
            // The renderer accepts 0 (BokehCreator filter), so the hard range includes it.
            min: 0.0,
            max: 10.0,
            default: 1.0,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.aspect_ratio = float(v),
    },
    ParamDef {
        name: "bokehNoiseSize",
        label: "Noise Size",
        hint: "Scale of the bokeh shape noise",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10.0,
            default: 0.1,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.noise.size = float(v),
    },
    ParamDef {
        name: "bokehNoiseIntensity",
        label: "Noise Intensity",
        hint: "Strength of the bokeh shape noise",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1.0,
            default: 0.25,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.noise.intensity = float(v),
    },
    ParamDef {
        name: "bokehNoiseOctaves",
        label: "Noise Octaves",
        hint: "Detail levels of the bokeh shape noise",
        kind: ParamKind::Int {
            min: 1,
            max: 16,
            default: 7,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.noise.octaves = int(v) as u32,
    },
    ParamDef {
        name: "bokehNoiseSeed",
        label: "Noise Seed",
        hint: "Seed of the bokeh shape noise",
        kind: ParamKind::Int {
            min: 0,
            max: 1000000,
            default: 0,
        },
        group: "bokeh",
        apply: |s, v| s.bokeh.noise.seed = int(v) as u32,
    },
    ParamDef {
        name: "catseyeEnable",
        label: "Enable",
        hint: "Cut the bokeh shape by the distance to the lens centre",
        kind: ParamKind::Bool { default: false },
        group: "catseye",
        apply: |s, v| s.non_uniform.catseye.enable = boolean(v),
    },
    ParamDef {
        name: "catseyeAmount",
        label: "Amount",
        hint: "Strength of the catseye effect",
        kind: ParamKind::Double {
            min: 0.0,
            max: 2.0,
            default: 0.5,
        },
        group: "catseye",
        apply: |s, v| s.non_uniform.catseye.amount = float(v),
    },
    ParamDef {
        name: "catseyeGamma",
        label: "Gamma",
        hint: "Gamma correction of the catseye effect",
        kind: ParamKind::Double {
            min: 0.01,
            max: 10.0,
            default: 1.0,
        },
        group: "catseye",
        apply: |s, v| s.non_uniform.catseye.gamma = float(v),
    },
    ParamDef {
        name: "catseyeSoftness",
        label: "Softness",
        hint: "Softness of the catseye edges",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1.0,
            default: 0.2,
        },
        group: "catseye",
        apply: |s, v| s.non_uniform.catseye.softness = float(v),
    },
    ParamDef {
        name: "catseyeInverse",
        label: "Inverse",
        hint: "Invert the catseye effect",
        kind: ParamKind::Bool { default: false },
        group: "catseye",
        apply: |s, v| s.non_uniform.catseye.inverse = boolean(v),
    },
    ParamDef {
        name: "axialAberrationEnable",
        label: "Enable",
        hint: "Colour fringing from light focusing at different distances",
        kind: ParamKind::Bool { default: false },
        group: "axialAberration",
        apply: |s, v| s.non_uniform.axial_aberration.enable = boolean(v),
    },
    ParamDef {
        name: "axialAberrationAmount",
        label: "Amount",
        hint: "Strength of the axial aberration",
        kind: ParamKind::Double {
            min: -10.0,
            max: 10.0,
            default: 0.5,
        },
        group: "axialAberration",
        apply: |s, v| s.non_uniform.axial_aberration.amount = float(v),
    },
    ParamDef {
        name: "axialAberrationColorType",
        label: "Colors",
        hint: "Colour pair of the axial aberration",
        kind: ParamKind::Choice {
            // Indexed by the raw `AxialAberrationType` value.
            options: &["Red / Blue", "Blue / Yellow", "Green / Purple"],
            default: AxialAberrationType::RedBlue as i32,
        },
        group: "axialAberration",
        apply: |s, v| s.non_uniform.axial_aberration.color_type = choice(v),
    },
    ParamDef {
        name: "astigmatismEnable",
        label: "Enable",
        hint: "Stretch the bokeh shape towards the image edges",
        kind: ParamKind::Bool { default: false },
        group: "astigmatism",
        apply: |s, v| s.non_uniform.astigmatism.enable = boolean(v),
    },
    ParamDef {
        name: "astigmatismAmount",
        label: "Amount",
        hint: "Strength of the astigmatism",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10.0,
            default: 0.5,
        },
        group: "astigmatism",
        apply: |s, v| s.non_uniform.astigmatism.amount = float(v),
    },
    ParamDef {
        name: "astigmatismGamma",
        label: "Gamma",
        hint: "Gamma correction of the astigmatism",
        kind: ParamKind::Double {
            min: 0.01,
            max: 10.0,
            default: 1.0,
        },
        group: "astigmatism",
        apply: |s, v| s.non_uniform.astigmatism.gamma = float(v),
    },
    ParamDef {
        name: "barndoorsEnable",
        label: "Enable",
        hint: "Block part of the aperture with matte boxes",
        kind: ParamKind::Bool { default: false },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.enable = boolean(v),
    },
    ParamDef {
        name: "barndoorsAmount",
        label: "Amount",
        hint: "Strength of the barndoors",
        kind: ParamKind::Double {
            min: 0.0,
            max: 10.0,
            default: 0.5,
        },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.amount = float(v),
    },
    ParamDef {
        name: "barndoorsGamma",
        label: "Gamma",
        hint: "Gamma correction of the barndoors",
        kind: ParamKind::Double {
            min: 0.01,
            max: 10.0,
            default: 1.0,
        },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.gamma = float(v),
    },
    ParamDef {
        name: "barndoorsInverse",
        label: "Inverse",
        hint: "Invert the barndoors effect",
        kind: ParamKind::Bool { default: false },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.inverse = boolean(v),
    },
    ParamDef {
        name: "barndoorsTop",
        label: "Top",
        hint: "Position of the top barndoor",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 100.0,
        },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.top = float(v),
    },
    ParamDef {
        name: "barndoorsBottom",
        label: "Bottom",
        hint: "Position of the bottom barndoor",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 100.0,
        },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.bottom = float(v),
    },
    ParamDef {
        name: "barndoorsLeft",
        label: "Left",
        hint: "Position of the left barndoor",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 100.0,
        },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.left = float(v),
    },
    ParamDef {
        name: "barndoorsRight",
        label: "Right",
        hint: "Position of the right barndoor",
        kind: ParamKind::Double {
            min: 0.0,
            max: 1000.0,
            default: 100.0,
        },
        group: "barndoors",
        apply: |s, v| s.non_uniform.barndoors.right = float(v),
    },
];
