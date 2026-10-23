//! Hardware parity comparison for the OFX plugin.
//!
//! Every full-frame entry of `test/parity/commands.tsv` is rendered through the plugin, hosted
//! by a minimal mock host that lives in this file. Inputs and settings are the ones the CLI
//! used; the baseline is the CLI output saved as EXR. Rows are fed bottom-up, as OFX hosts
//! deliver them, so the row flip is exercised. The max absolute difference must stay within
//! [`TOLERANCE`].
//!
//! Entries whose settings the parameter table cannot express are skipped with a printed reason.
//! The tolerance is never relaxed for them.
//!
//! The test needs a GPU and the baselines, which the CLI of this checkout renders on the same
//! machine because baselines differ between GPUs. Run both steps with:
//!
//! ```sh
//! cargo xtask parity
//! ```

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test helpers fail loudly with a panic; only #[test] functions are exempt from these lints by clippy.toml"
)]
#![allow(
    clippy::too_many_arguments,
    reason = "the parity harness mirrors the plugin entry points with their full argument lists"
)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};
use std::ptr;
use std::sync::{Mutex, OnceLock, PoisonError};

use opendefocus::datamodel::Settings;
use opendefocus::datamodel::render::Quality;
use opendefocus_ofx::ofx_sys::{
    OFX_ACTION_CREATE_INSTANCE, OFX_ACTION_DESTROY_INSTANCE, OFX_ACTION_LOAD, OFX_ACTION_UNLOAD,
    OFX_BIT_DEPTH_FLOAT, OFX_IMAGE_CLIP_PROP_CONNECTED, OFX_IMAGE_COMPONENT_RGBA,
    OFX_IMAGE_EFFECT_ACTION_RENDER, OFX_IMAGE_EFFECT_PROP_COMPONENTS,
    OFX_IMAGE_EFFECT_PROP_PIXEL_DEPTH, OFX_IMAGE_EFFECT_PROP_RENDER_WINDOW, OFX_IMAGE_EFFECT_SUITE,
    OFX_IMAGE_PROP_BOUNDS, OFX_IMAGE_PROP_DATA, OFX_IMAGE_PROP_REGION_OF_DEFINITION,
    OFX_IMAGE_PROP_ROW_BYTES, OFX_PARAMETER_SUITE, OFX_PROP_TIME, OFX_PROPERTY_SUITE,
    OFX_STAT_ERR_BAD_HANDLE, OFX_STAT_ERR_UNKNOWN, OFX_STAT_ERR_VALUE, OFX_STAT_FAILED,
    OFX_STAT_OK, OfxHost, OfxImageClipHandle, OfxImageEffectHandle, OfxImageEffectSuiteV1,
    OfxParamHandle, OfxParamSetHandle, OfxParameterSuiteV1, OfxPlugin, OfxPropertySetHandle,
    OfxPropertySuiteV1, OfxRectD, OfxStatus, OfxTime,
};
use opendefocus_ofx::params::{PARAMS, ParamKind, ParamValue};
use opendefocus_ofx::plugin::OfxGetPlugin;
use serde_json::Value;

/// Largest accepted absolute difference per channel. Never relaxed.
const TOLERANCE: f32 = 1e-6;

mod entries;
mod harness;
mod host;
mod settings;
mod suites;

use entries::*;
use harness::*;
use host::*;
use settings::*;
use suites::*;

#[test]
#[ignore = "needs a GPU and the baselines; run `cargo xtask parity`"]
fn parity_scenes_match_cli_baseline() {
    let parity = parity_dir();
    let repo = repo_root();
    let entries = parity_entries(&parity, &repo);
    assert!(!entries.is_empty(), "no full-frame entries in commands.tsv");

    let mut rows = Vec::new();
    let mut failures = Vec::new();
    let mut skipped = 0;
    for entry in &entries {
        let settings_text = std::fs::read_to_string(&entry.settings).expect("settings JSON");
        let settings: Value = serde_json::from_str(&settings_text).expect("valid settings JSON");
        let values = match entry_params(entry, &settings) {
            Ok(values) => values,
            Err(reasons) => {
                skipped += 1;
                eprintln!("SKIP {}: {}", entry.name, reasons.join("; "));
                continue;
            }
        };

        let color = load_frame(&entry.input).expect("input colour image");
        let depth = entry
            .depth
            .as_ref()
            .map(|path| load_frame(path).expect("depth image"));
        let rendered = match render_through_plugin(values, &color, depth.as_ref()) {
            Ok(rendered) => rendered,
            Err(reason) => {
                failures.push(format!("{}: {reason}", entry.name));
                continue;
            }
        };

        let baseline_path = baseline_dir().join(format!("{}.exr", entry.name));
        let baseline = load_frame(&baseline_path).expect("baseline EXR");
        if baseline.width != color.width || baseline.height != color.height {
            failures.push(format!(
                "{}: baseline is {}x{}, render is {}x{}",
                entry.name, baseline.width, baseline.height, color.width, color.height
            ));
            continue;
        }
        let (max, non_finite) = compare(&rendered, &baseline.rows);
        rows.push(format!(
            "{:<36} max_abs_diff={max:.3e} non_finite={non_finite}",
            entry.name
        ));
        if non_finite > 0 || max > TOLERANCE {
            failures.push(format!(
                "{}: max abs diff {max:e} (non-finite {non_finite}) exceeds {TOLERANCE:e}",
                entry.name
            ));
        }
    }

    for row in &rows {
        println!("{row}");
    }
    println!("compared {} scenes, skipped {skipped}", rows.len());
    assert_eq!(rows.len(), 12, "expected every full-render parity scene");
    assert_eq!(skipped, 0, "no parity scene may be skipped");
    assert!(
        failures.is_empty(),
        "parity mismatches:\n{}",
        failures.join("\n")
    );
}
