//! Instance state, the Render action, and the copies between OFX images and arrays.
//!
//! OFX rows run bottom-up (row `y1` is the bottom row) and `rowBytes` may be negative. The
//! renderer works on top-down `(height, width, channels)` arrays, so every copy flips rows.
//! Tiles: `GetRegionsOfInterest` asks for the window grown by the renderer's padding (clamped to
//! the Source region of definition), and Render gives the renderer that padded context with
//! full-frame coordinates, then writes only the window.

use core::ffi::c_void;
use std::ffi::CString;

use ndarray::{Array2, Array3, Axis, s};
use opendefocus::OpenDefocusRenderer;
use opendefocus::datamodel::defocus::DefocusMode;
use opendefocus::datamodel::render::RenderSpecs;
use opendefocus::datamodel::{IVector4, Settings, UVector2, Vector2};

use crate::camera::read_connected_camera;
use crate::ofx_sys::{
    OFX_IMAGE_CLIP_PROP_CONNECTED, OFX_IMAGE_CLIP_PROP_ROI_PREFIX,
    OFX_IMAGE_EFFECT_PROP_COMPONENTS, OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH,
    OFX_IMAGE_EFFECT_PROP_REGION_OF_INTEREST, OFX_IMAGE_EFFECT_PROP_RENDER_SCALE,
    OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW, OFX_IMAGE_PROP_BOUNDS, OFX_IMAGE_PROP_DATA,
    OFX_IMAGE_PROP_PIXEL_ASPECT_RATIO, OFX_IMAGE_PROP_REGION_OF_DEFINITION,
    OFX_IMAGE_PROP_ROW_BYTES, OFX_PROP_INSTANCE_DATA, OFX_PROP_TIME, OFX_STAT_FAILED, OFX_STAT_OK,
    OfxImageClipHandle, OfxImageEffectHandle, OfxPropertySetHandle, OfxRectD, OfxStatus, OfxTime,
};
use crate::params::read_settings_with_camera;
use crate::plugin::effect_props;
use crate::suites::{
    HostSuites, OfxError, Props, Result as HostResult, c_string, message_error, stored_suites,
};

/// Name of the input clip that carries the image to defocus.
const CLIP_SOURCE: &core::ffi::CStr = c"Source";
/// Name of the optional clip that carries the depth map.
const CLIP_DEPTH: &core::ffi::CStr = c"Depth";
/// Name of the output clip the render writes into.
const CLIP_OUTPUT: &core::ffi::CStr = c"Output";

/// Pixel depth the plugin describes and renders: 32-bit float per channel.
const BIT_DEPTH_FLOAT: &str = "OfxBitDepthFloat";
/// Component set for four-channel images with alpha.
const COMPONENTS_RGBA: &str = "OfxImageComponentRGBA";
/// Component set for single-channel alpha images.
const COMPONENTS_ALPHA: &str = "OfxImageComponentAlpha";

mod action;
mod clip;
mod failure;
mod geometry;
mod images;
mod instance;
mod regions;
mod settings;
#[cfg(test)]
mod tests;

pub use action::*;
use clip::{clip_handles, clip_region_of_definition, read_doubles, read_ints, render_window};
use failure::{Failure, Outcome, fail, report, with_context};
use geometry::{
    PixelMap, Rect, clip_pixel_aspect, intersect, reads_remote_source, rect_text, render_scale,
    set_frame, source_request, top_down,
};
use images::{
    Image, check_abort, check_image, clear_window, fetch_image, read_window, write_window,
};
pub use instance::*;
pub use regions::*;
pub use settings::*;
