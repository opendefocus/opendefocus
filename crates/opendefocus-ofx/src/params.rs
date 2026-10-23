//! Table-driven parameter definitions.
//!
//! Adding a parameter means adding one row to [`PARAMS`]. The row carries the
//! host-facing definition (type, range, default, choices, group) and the function
//! that writes its value into [`Settings`]. Defaults are taken from the table, so
//! [`default_settings`] equals `Settings::default()` as long as the rows agree.

use opendefocus::datamodel::Settings;
use opendefocus::datamodel::circle_of_confusion::{CameraData, Math, WorldUnit};
use opendefocus::datamodel::non_uniform::AxialAberrationType;
use opendefocus::datamodel::render::{FilterMode, Quality};

use core::ffi::c_int;

use crate::camera::NukeCamera;
use crate::ofx_sys::{
    OFX_PARAM_PROP_ANIMATES, OFX_PARAM_PROP_CHOICE_OPTION, OFX_PARAM_PROP_DEFAULT,
    OFX_PARAM_PROP_DISPLAY_MAX, OFX_PARAM_PROP_DISPLAY_MIN, OFX_PARAM_PROP_ENABLED,
    OFX_PARAM_PROP_HINT, OFX_PARAM_PROP_MAX, OFX_PARAM_PROP_MIN, OFX_PARAM_PROP_PAGE_CHILD,
    OFX_PARAM_PROP_PARENT, OFX_PARAM_TYPE_BOOLEAN, OFX_PARAM_TYPE_CHOICE, OFX_PARAM_TYPE_DOUBLE,
    OFX_PARAM_TYPE_GROUP, OFX_PARAM_TYPE_INTEGER, OFX_PARAM_TYPE_PAGE,
    OFX_PLUGIN_PROP_PARAM_PAGE_ORDER, OFX_PROP_LABEL, OfxParamHandle, OfxParamSetHandle,
    OfxPropertySetHandle, OfxTime,
};
use crate::suites::{
    HostSuites, Props, Result, c_string, check, param_get_bool, param_get_f64, param_get_i32,
    stored_suites,
};

mod define;
mod enable;
mod layout;
mod ranges;
mod settings;
mod table;
#[cfg(test)]
mod tests;
mod values;

pub use define::*;
pub use enable::*;
pub use layout::*;
pub use ranges::*;
pub use settings::*;
pub use table::*;
pub use values::*;
