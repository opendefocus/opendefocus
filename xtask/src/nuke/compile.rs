use std::path::PathBuf;

use anyhow::Result;
use duct::cmd;

use crate::{
    TargetPlatform,
    nuke::sources::{dll_prefix, dll_suffix, get_sources, nuke_source_directory},
    util::{crate_root, path_to_string, target_directory},
};

fn build_dir(version: &str, target: &TargetPlatform) -> PathBuf {
    target_directory()
        .join("nuke")
        .join("builds")
        .join(version)
        .join(format!("{}", target))
}

pub async fn compile_nuke(
    versions: Vec<String>,
    target: TargetPlatform,
    limit_threads: bool,
) -> Result<()> {
    get_sources(vec![target], versions.clone(), limit_threads).await?;

    for version in versions {
        if target == TargetPlatform::MacosAarch64 || target == TargetPlatform::MacosX86_64 {
            unsafe {
                std::env::set_var(
                    "MACOSX_DEPLOYMENT_TARGET",
                    get_macos_deployment_target(&version)?,
                )
            };
        };
        let cpp_version = get_cpp_version(&version)?;
        println!("cargo:rustc-env=CPP_VERSION={}", cpp_version);
        compile(&version, &target).await?
    }

    Ok(())
}

fn get_cpp_version(version: &str) -> Result<usize> {
    let parsed_version = version.parse::<f32>()?;
    if parsed_version < 13.0 {
        return Ok(11);
    } else if parsed_version < 14.0 {
        return Ok(14);
    }

    Ok(17)
}

fn get_macos_deployment_target(version: &str) -> Result<String> {
    let parsed_version = version.parse::<f32>()?;
    if parsed_version < 13.0 {
        return Ok("11.0".to_owned());
    } else if parsed_version < 14.0 {
        return Ok("12.0".to_owned());
    }

    Ok("14".to_owned())
}

async fn compile(version: &str, target: &TargetPlatform) -> Result<(), anyhow::Error> {
    let sources_directory = nuke_source_directory(version);
    let crates_path = path_to_string(
        &crate_root()
            .join("crates")
            .join("opendefocus-nuke")
            .join("Cargo.toml"),
    )?;
    cmd!(
        "cargo",
        "build",
        "--manifest-path",
        &crates_path,
        "--release",
    )
    .env("NUKE_SOURCE_PATH", &sources_directory)
    .run()?;
    println!(
        "cargo:rustc-env=NUKE_SOURCE_PATH={}",
        path_to_string(&sources_directory)?
    );
    let dylib = dll_suffix(*target);
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!("OpenDefocus.{}", dll_suffix(*target)));
    let build_lib = path_to_string(
        &&target_directory()
            .join("release")
            .join(format!("{}opendefocus_nuke.{dylib}", dll_prefix(*target))),
    )?;
    tokio::fs::rename(build_lib, output_dylib).await?;
    Ok(())
}
