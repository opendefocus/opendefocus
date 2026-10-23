//! Mock OFX host for testing the plugin rlib without a real host application.
//!
//! The mock implements the six suites the plugin fetches. Properties live in one process-wide
//! map keyed by (property set handle, name). Handles are small integers cast to pointers.
//! The plugin keeps its host and suites in process-wide state, so every test takes a
//! [`Session`], which serializes tests on a lock and clears the mock state.
//!
//! # Variadic host functions
//!
//! Stable Rust can call C-variadic function pointers but cannot define C-variadic functions.
//! The mock therefore stores ordinary functions in the variadic pointer slots, through
//! `transmute`. This is only sound because the receivers ignore the variadic arguments
//! they cannot read, or read exactly the one trailing argument the plugin passes:
//!
//! - Targets where a variadic argument is passed like a named one (x86-64 `SysV` and Win64,
//!   AAPCS64 on Linux and Windows arm64): the trailing pointer is in the same register or
//!   stack slot as a named parameter, so an ordinary receiver with the same parameters reads
//!   it correctly. Those receivers are used directly.
//! - Apple arm64: every variadic argument is passed on the stack, after the named arguments
//!   that occupy `x0..x7`. The receivers therefore declare unused integer parameters that fill
//!   `x1..x7` (and `x4..x7` for the message suite), which pushes the trailing parameter to the
//!   first stack slot, where the caller put it.
//!
//! The `paramGetValue` family receives a pointer to the value to write. The mock writes the
//! stored value using the parameter's own type, so a pointer of the wrong type from a caller
//! would corrupt memory; the plugin helpers always pass the type the header defines.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly with a panic; only #[test] functions are exempt from these lints by clippy.toml"
)]
#![allow(
    clippy::too_many_arguments,
    reason = "the mock receivers mirror the C signatures of the OFX suites"
)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::mem::transmute;
use std::ptr::{self, NonNull};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

use opendefocus::datamodel::Settings;
use opendefocus::datamodel::circle_of_confusion::CameraData;
use opendefocus_ofx::ofx_sys::{
    NUKE_OFX_CAMERA_PARAM_FOCAL_LENGTH, NUKE_OFX_CAMERA_PARAM_HORIZONTAL_APERTURE,
    NUKE_OFX_CAMERA_PARAM_VERTICAL_APERTURE, NUKE_OFX_CAMERA_SUITE, NukeOfxCameraHandle,
    NukeOfxCameraSuiteV1,
};
use opendefocus_ofx::ofx_sys::{
    OFX_ACTION_CREATE_INSTANCE, OFX_ACTION_DESCRIBE, OFX_ACTION_DESTROY_INSTANCE,
    OFX_ACTION_INSTANCE_CHANGED, OFX_ACTION_LOAD, OFX_ACTION_UNLOAD, OFX_IMAGE_CLIP_PROP_OPTIONAL,
    OFX_IMAGE_EFFECT_ACTION_DESCRIBE_IN_CONTEXT, OFX_IMAGE_EFFECT_ACTION_GET_REGIONS_OF_INTEREST,
    OFX_IMAGE_EFFECT_ACTION_RENDER, OFX_IMAGE_EFFECT_CONTEXT_FILTER, OFX_IMAGE_EFFECT_PROP_CONTEXT,
    OFX_IMAGE_EFFECT_PROP_REGION_OF_INTEREST, OFX_IMAGE_EFFECT_SUITE, OFX_IMAGE_PROP_BOUNDS,
    OFX_IMAGE_PROP_DATA, OFX_IMAGE_PROP_ROW_BYTES, OFX_MEMORY_SUITE, OFX_MESSAGE_SUITE,
    OFX_MULTI_THREAD_SUITE, OFX_PARAM_PROP_CHOICE_OPTION, OFX_PARAM_PROP_DEFAULT,
    OFX_PARAM_PROP_DISPLAY_MAX, OFX_PARAM_PROP_DISPLAY_MIN, OFX_PARAM_PROP_ENABLED,
    OFX_PARAM_PROP_PAGE_CHILD, OFX_PARAM_PROP_PARENT, OFX_PARAMETER_SUITE,
    OFX_PLUGIN_PROP_PARAM_PAGE_ORDER, OFX_PROP_LABEL, OFX_PROP_TYPE, OFX_PROPERTY_SUITE,
    OFX_STAT_ERR_BAD_HANDLE, OFX_STAT_ERR_BAD_INDEX, OFX_STAT_ERR_EXISTS, OFX_STAT_ERR_MEMORY,
    OFX_STAT_ERR_MISSING_HOST_FEATURE, OFX_STAT_ERR_UNKNOWN, OFX_STAT_ERR_VALUE, OFX_STAT_FAILED,
    OFX_STAT_OK, OFX_TYPE_PARAMETER, OfxHost, OfxImageClipHandle, OfxImageEffectHandle,
    OfxImageEffectSuiteV1, OfxImageMemoryHandle, OfxMemorySuiteV1, OfxMessageSuiteV1,
    OfxMultiThreadSuiteV1, OfxMutex, OfxParamHandle, OfxParamSetHandle, OfxParameterSuiteV1,
    OfxPlugin, OfxPropertySetHandle, OfxPropertySuiteV1, OfxRangeD, OfxRectD, OfxStatus, OfxTime,
};
use opendefocus_ofx::ofx_sys::{
    OFX_IMAGE_CLIP_PROP_COMPONENTS_OUTPUT, OFX_IMAGE_CLIP_PROP_CONNECTED,
    OFX_IMAGE_CLIP_PROP_DEPTH_OUTPUT, OFX_IMAGE_EFFECT_ACTION_GET_CLIP_PREFERENCES,
    OFX_IMAGE_EFFECT_PROP_COMPONENTS, OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH,
    OFX_IMAGE_EFFECT_PROP_PRE_MULTIPLICATION, OFX_IMAGE_EFFECT_PROP_RENDER_SCALE,
    OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW, OFX_IMAGE_PROP_PIXEL_ASPECT_RATIO,
    OFX_IMAGE_PROP_REGION_OF_DEFINITION, OFX_PROP_INSTANCE_DATA, OFX_PROP_TIME,
};
use opendefocus_ofx::params::{
    DISPLAY_RANGES, GROUPS, PAGES, PARAMS, ParamKind, default_settings, page_of, read_settings,
};
use opendefocus_ofx::plugin::OfxGetPlugin;
use opendefocus_ofx::render::effect_settings;

mod camera_faults;
mod camera_input;
mod camera_suite;
mod entry_points;
mod fault_sweep;
mod hardware;
mod host;
mod image_effect_suite;
mod lifecycle;
mod memory_thread_message;
mod parameter_pages;
mod parameter_suite;
mod property_suite;
mod reduced_scale;
mod regions_checks;
mod render_action;
mod render_faults;
mod render_guards;
pub mod session;
mod state;
mod tiles;

use camera_suite::*;
use hardware::*;
use host::*;
use image_effect_suite::*;
use lifecycle::*;
use memory_thread_message::*;
use parameter_suite::*;
use property_suite::*;
use render_action::*;
use session::*;
use state::*;
use tiles::*;
