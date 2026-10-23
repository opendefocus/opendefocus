//! Builds the standalone CLI and UI, packages the WebAssembly viewer and validates the
//! native WGSL shaders with naga.
use anyhow::{Context, Result, ensure};
use duct::cmd;

use crate::util::{crate_root, target_directory};

/// Builds the `opendefocus-cli` package in release mode with `--locked`. Fails when cargo
/// fails.
pub fn compile_cli() -> Result<()> {
    let args = vec![
        "build",
        "--release",
        "--locked",
        "--package",
        "opendefocus-cli",
    ];
    cmd("cargo", args).dir(crate_root()).run()?;
    log::info!("Standalone application built: opendefocus-cli in Cargo's release directory");
    Ok(())
}

/// Builds the `opendefocus-ui` package in release mode with `--locked`. Fails when cargo
/// fails.
pub fn compile_ui() -> Result<()> {
    let args = vec![
        "build",
        "--release",
        "--locked",
        "--package",
        "opendefocus-ui",
    ];
    cmd("cargo", args).dir(crate_root()).run()?;
    log::info!("Standalone application built: opendefocus-ui in Cargo's release directory");
    Ok(())
}

/// Builds the `opendefocus-ui` package in release mode with `--locked` and runs it with `arguments`. Fails when cargo fails or the viewer exits with an error.
pub fn run_ui(arguments: &[String]) -> Result<()> {
    let mut args = vec![
        "run",
        "--release",
        "--locked",
        "--package",
        "opendefocus-ui",
    ];
    if !arguments.is_empty() {
        args.push("--");
        args.extend(arguments.iter().map(String::as_str));
    }
    cmd("cargo", args).dir(crate_root()).run()?;
    Ok(())
}

/// Package the same viewer for a static HTTP server, including GitHub Pages.
pub fn compile_web() -> Result<()> {
    cmd("wasm-bindgen", ["--version"]).run().context(
        "Install wasm-bindgen-cli 0.2.129 (matching Cargo.lock) before building for web",
    )?;
    let args = vec![
        "build",
        "--release",
        "--locked",
        "--package",
        "opendefocus-ui",
        "--lib",
        "--target",
        "wasm32-unknown-unknown",
    ];
    cmd("cargo", args).dir(crate_root()).run()?;
    let output = target_directory().join("web");
    std::fs::create_dir_all(&output)?;
    cmd!(
        "wasm-bindgen",
        "--target",
        "web",
        "--out-name",
        "opendefocus",
        "--out-dir",
        &output,
        target_directory().join("wasm32-unknown-unknown/release/opendefocus_ui.wasm")
    )
    .run()?;
    std::fs::copy(
        crate_root().join("crates/opendefocus-ui/web/index.html"),
        output.join("index.html"),
    )?;
    let standalone = standalone_html(
        &std::fs::read_to_string(output.join("index.html"))?,
        &std::fs::read_to_string(output.join("opendefocus.js"))?,
        &std::fs::read(output.join("opendefocus_bg.wasm"))?,
    )?;
    std::fs::write(output.join("opendefocus-evaluation.html"), standalone)?;
    std::fs::write(output.join(".nojekyll"), "")?;
    log::info!("WebAssembly viewer built: {}", output.display());
    Ok(())
}

/// Validate authored GPU programs without a nightly shader-generation step.
pub async fn validate_gpu_shaders() -> Result<()> {
    let root = crate_root().join("crates/opendefocus/src/kernel/shaders");
    for name in [
        "adaptive_classifier.wgsl",
        "adaptive_prepare.wgsl",
        "adaptive_pyramid.wgsl",
        "adaptive_convolution.wgsl",
        "adaptive_preview.wgsl",
    ] {
        let source = tokio::fs::read_to_string(root.join(name)).await?;
        let module = naga::front::wgsl::parse_str(&source)
            .map_err(|error| anyhow::anyhow!("{}", error.emit_to_string(&source)))
            .with_context(|| format!("GPU shader {name}"))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| anyhow::anyhow!("{}", error.emit_to_string(&source)))
        .with_context(|| format!("GPU shader {name}"))?;
        write_spirv_for_every_entry_point(&module, name)?;
        log::info!("Validated GPU shader {name}");
    }
    Ok(())
}

/// Writes SPIR-V for every compute entry point of `module`, the form wgpu hands to Vulkan.
/// Metal and the WGSL validator accept shaders that naga's SPIR-V writer rejects or panics
/// on, so this catches Linux and Windows failures without a Vulkan adapter. Fails when an
/// entry point cannot be written.
fn write_spirv_for_every_entry_point(module: &naga::Module, name: &str) -> Result<()> {
    for entry_point in &module.entry_points {
        let written = std::panic::catch_unwind(|| {
            let info = naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(module)
            .map_err(|error| anyhow::anyhow!("{error}"))?;
            let constants = naga::back::PipelineConstants::default();
            let (module, info) = naga::back::pipeline_constants::process_overrides(
                module,
                &info,
                Some((entry_point.stage, entry_point.name.as_str())),
                &constants,
            )
            .map_err(|error| anyhow::anyhow!("{error}"))?;
            let pipeline = naga::back::spv::PipelineOptions {
                shader_stage: entry_point.stage,
                entry_point: entry_point.name.clone(),
            };
            naga::back::spv::write_vec(
                &module,
                &info,
                &naga::back::spv::Options::default(),
                Some(&pipeline),
            )
            .map(drop)
            .map_err(|error| anyhow::anyhow!("{error}"))
        });
        match written {
            Ok(result) => result.with_context(|| {
                format!(
                    "SPIR-V for GPU shader {name}, entry point {}",
                    entry_point.name
                )
            })?,
            Err(_) => anyhow::bail!(
                "naga panicked writing SPIR-V for GPU shader {name}, entry point {}",
                entry_point.name
            ),
        }
    }
    Ok(())
}

/// Builds one self-contained HTML page by inlining the wasm-bindgen loader and the
/// WebAssembly bytes. The template must contain the loader import and the `init` call;
/// otherwise an error is returned. The bytes are embedded as a JSON array of code units so
/// they survive unchanged.
fn standalone_html(template: &str, js: &str, wasm: &[u8]) -> Result<String> {
    ensure!(
        template.contains("import('./opendefocus.js')") && template.contains("await init();"),
        "Web loader changed; update standalone packaging"
    );
    // JSON preserves every byte; escape '<' so embedded text cannot close the script tag.
    let js = serde_json::to_string(js)?.replace('<', "\\u003c");
    let wasm: String = wasm.iter().copied().map(char::from).collect();
    let wasm = serde_json::to_string(&wasm)?.replace('<', "\\u003c");
    Ok(template
        .replace(
            "import('./opendefocus.js')",
            &format!(
                "import(URL.createObjectURL(new Blob([{js}], {{ type: 'text/javascript' }})))"
            ),
        )
        .replace(
            "await init();",
            &format!(
                "await init({{ module_or_path: Uint8Array.from({wasm}, c => c.charCodeAt(0)) }});"
            ),
        ))
}

/// Tests for the standalone HTML packaging.
#[cfg(test)]
mod web_tests {
    use super::*;

    /// Verifies that the inlined bytes decode back to the originals and that `</script>` cannot
    /// appear in the output. Also verifies that a changed template is rejected.
    #[test]
    fn test_standalone_preserves_binary_bytes_and_cannot_close_script() -> Result<()> {
        let bytes = [0, 10, b'"', b'\\', b'<', 128, 255];
        let template = "const app = import('./opendefocus.js'); await init();";
        let html = standalone_html(template, "export default '</script>';", &bytes)?;
        assert!(!html.contains("</script>"));
        assert!(!html.contains("./opendefocus.js"));
        let literal = html
            .split("Uint8Array.from(")
            .nth(1)
            .unwrap()
            .split(", c =>")
            .next()
            .unwrap();
        let decoded: String = serde_json::from_str(literal)?;
        assert_eq!(decoded.chars().map(|c| c as u8).collect::<Vec<_>>(), bytes);
        assert!(standalone_html("changed template", "", &[]).is_err());
        Ok(())
    }
}
