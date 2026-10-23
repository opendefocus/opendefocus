//! Native Python bindings (`python-bindings` feature).

use pyo3::prelude::*;

use crate::{Calculator, CameraData, Filmback, Math, Resolution, Settings, WorldUnit};

/// Declares a frozen Python enum `$py` named `$name` that mirrors the Rust enum `$rust`.
///
/// It also generates conversions in both directions. Each variant `$v` maps to the Rust
/// variant with the same name.
macro_rules! py_enum {
    ($py:ident, $rust:ident, $name:literal, $($v:ident),+) => {
        #[pyclass(name = $name, eq, eq_int, frozen, from_py_object)]
        #[derive(Clone, Copy, PartialEq)]
        enum $py { $(#[doc = concat!("Python value for `", stringify!($v), "`.")] $v),+ }
        impl From<$py> for $rust {
            fn from(v: $py) -> Self { match v { $($py::$v => $rust::$v),+ } }
        }
        impl From<$rust> for $py {
            fn from(v: $rust) -> Self { match v { $($rust::$v => $py::$v),+ } }
        }
    };
}

py_enum!(PyWorldUnit, WorldUnit, "WorldUnit", M, Mm, Cm, Dm, In, Ft);
py_enum!(PyMath, Math, "Math", OneDividedByZ, Real);

/// Python wrapper for [`Filmback`](crate::Filmback), the sensor size in millimeters.
#[pyclass(name = "Filmback", get_all, set_all, from_py_object)]
#[derive(Clone)]
struct PyFilmback {
    /// Sensor width.
    width: f32,
    /// Sensor height.
    height: f32,
}

#[pymethods]
impl PyFilmback {
    /// Creates a filmback, defaulting to the Rust default size.
    #[new]
    #[pyo3(signature = (width=Filmback::default().width, height=Filmback::default().height))]
    fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

/// Python wrapper for [`Resolution`](crate::Resolution), the recording size in pixels.
#[pyclass(name = "Resolution", get_all, set_all, from_py_object)]
#[derive(Clone)]
struct PyResolution {
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
}

#[pymethods]
impl PyResolution {
    /// Creates a resolution, defaulting to the Rust default size.
    #[new]
    #[pyo3(signature = (width=Resolution::default().width, height=Resolution::default().height))]
    fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }
}

/// Python wrapper for [`CameraData`](crate::CameraData): lens, sensor and recording parameters.
#[pyclass(name = "CameraData", get_all, set_all, from_py_object)]
#[derive(Clone)]
struct PyCameraData {
    /// Focal length of the lens.
    focal_length: f32,
    /// Aperture as an f-number.
    f_stop: f32,
    /// Sensor size.
    filmback: PyFilmback,
    /// Near field distance for depth of field.
    near_field: f32,
    /// Far field distance for depth of field.
    far_field: f32,
    /// Unit of the depth values.
    world_unit: PyWorldUnit,
    /// Recording resolution.
    resolution: PyResolution,
}

#[pymethods]
impl PyCameraData {
    /// Creates camera data; omitted sensor or resolution values use the Rust defaults.
    #[new]
    #[pyo3(signature = (focal_length=50.0, f_stop=16.0, filmback=None, near_field=0.1,
        far_field=10000.0, world_unit=PyWorldUnit::M, resolution=None))]
    fn new(
        focal_length: f32,
        f_stop: f32,
        filmback: Option<PyFilmback>,
        near_field: f32,
        far_field: f32,
        world_unit: PyWorldUnit,
        resolution: Option<PyResolution>,
    ) -> Self {
        let d = CameraData::default();
        Self {
            focal_length,
            f_stop,
            filmback: filmback.unwrap_or(PyFilmback::new(d.filmback.width, d.filmback.height)),
            near_field,
            far_field,
            world_unit,
            resolution: resolution
                .unwrap_or(PyResolution::new(d.resolution.width, d.resolution.height)),
        }
    }
}

impl From<&PyCameraData> for CameraData {
    fn from(c: &PyCameraData) -> Self {
        Self {
            focal_length: c.focal_length,
            f_stop: c.f_stop,
            filmback: Filmback {
                width: c.filmback.width,
                height: c.filmback.height,
            },
            near_field: c.near_field,
            far_field: c.far_field,
            world_unit: WorldUnit::from(c.world_unit) as i32,
            resolution: Resolution {
                width: c.resolution.width,
                height: c.resolution.height,
            },
        }
    }
}

/// Python wrapper for [`Settings`](crate::Settings) used to build a [`Calculator`](crate::Calculator).
#[pyclass(name = "Settings", get_all, set_all, from_py_object)]
#[derive(Clone)]
struct PySettings {
    /// Base circle of confusion size.
    size: f32,
    /// Maximum circle of confusion size, used for clamping.
    max_size: f32,
    /// Interpretation of the depth values.
    math: PyMath,
    /// Depth of the focal plane.
    focal_plane: f32,
    /// Width of the protected in-focus range.
    protect: f32,
    /// Pixel aspect ratio applied to the result.
    pixel_aspect: f32,
    /// Physical camera parameters, or `None` for the direct map.
    camera_data: Option<PyCameraData>,
}

#[pymethods]
impl PySettings {
    /// Creates settings; omitted values use the defaults shown in the signature.
    #[new]
    #[pyo3(signature = (size=5.0, max_size=10.0, math=PyMath::OneDividedByZ, focal_plane=0.0,
        protect=0.0, pixel_aspect=1.0, camera_data=None))]
    fn new(
        size: f32,
        max_size: f32,
        math: PyMath,
        focal_plane: f32,
        protect: f32,
        pixel_aspect: f32,
        camera_data: Option<PyCameraData>,
    ) -> Self {
        Self {
            size,
            max_size,
            math,
            focal_plane,
            protect,
            pixel_aspect,
            camera_data,
        }
    }
}

impl From<&PySettings> for Settings {
    fn from(s: &PySettings) -> Self {
        Self {
            size: s.size,
            max_size: s.max_size,
            math: Math::from(s.math) as i32,
            focal_plane: s.focal_plane,
            protect: s.protect,
            pixel_aspect: s.pixel_aspect,
            camera_data: s.camera_data.as_ref().map(CameraData::from),
        }
    }
}

/// Python wrapper for [`Calculator`](crate::Calculator), precomputed from the settings.
#[pyclass(name = "Calculator", frozen)]
struct PyCalculator(Calculator);

#[pymethods]
impl PyCalculator {
    /// Precomputes a calculator from `settings`.
    #[new]
    fn new(settings: &PySettings) -> Self {
        Self(Calculator::new(Settings::from(settings)))
    }

    /// Calculate the circle of confusion size for a depth value.
    fn calculate(&self, value: f32) -> f32 {
        self.0.calculate(value)
    }

    /// Calculate the circle of confusion size for each depth value.
    fn calculate_many(&self, values: Vec<f32>) -> Vec<f32> {
        values.into_iter().map(|v| self.0.calculate(v)).collect()
    }
}

/// Native module `_native`, registered with the Python interpreter.
///
/// # Errors
///
/// Returns an error if a class cannot be added to the module.
#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyWorldUnit>()?;
    m.add_class::<PyMath>()?;
    m.add_class::<PyFilmback>()?;
    m.add_class::<PyResolution>()?;
    m.add_class::<PyCameraData>()?;
    m.add_class::<PySettings>()?;
    m.add_class::<PyCalculator>()?;
    Ok(())
}
