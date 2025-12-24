use std::path::{Path, PathBuf};

use anyhow::{Error, Result};
use duct::cmd;

use crate::{
    TargetPlatform,
    nuke::sources::{dll_prefix, dll_suffix, get_sources, nuke_source_directory},
    util::{crate_root, path_to_string, target_directory},
};

fn build_dir(version: &str, target: TargetPlatform) -> PathBuf {
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
    use_zig: bool,
) -> Result<()> {
    get_sources(vec![target], versions.clone(), limit_threads).await?;

    for version in versions {
        if !nuke_source_directory(&version)
            .join(format!(
                "{}DDImage.{}",
                dll_prefix(target),
                dll_suffix(target)
            ))
            .exists()
        {
            log::warn!("Skipping {version} as no sources could be found for this version on {target}");
            continue;
        };
        if target == TargetPlatform::MacosAarch64 || target == TargetPlatform::MacosX86_64 {
            unsafe {
                std::env::set_var(
                    "MACOSX_DEPLOYMENT_TARGET",
                    get_macos_deployment_target(&version)?,
                )
            };
        };

        if use_zig {
            compile_zig(&version, target).await?;
            continue;
        }
        if is_crosscompiling(target) {
            return Err(Error::msg("Cant cross compile without using zig or xwin"));
        }

        compile_native(&version, target).await?
    }

    Ok(())
}

fn is_crosscompiling(target: TargetPlatform) -> bool {
    match target {
        TargetPlatform::Linux => std::env::consts::OS != "linux",
        TargetPlatform::Windows => std::env::consts::OS != "windows",
        TargetPlatform::MacosAarch64 => {
            std::env::consts::OS != "macos" && std::env::consts::ARCH != "aarch64"
        }
        TargetPlatform::MacosX86_64 => {
            std::env::consts::OS != "macos" && std::env::consts::ARCH != "x86_64"
        }
    }
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

fn use_cxx11_abi(version: &str) -> Result<bool> {
    let parsed_version = version.parse::<f32>()?;

    Ok(parsed_version >= 14.1)
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

fn get_platform_name(target: TargetPlatform) -> String {
    match target {
        TargetPlatform::Linux => "linux",
        TargetPlatform::Windows => "windows",
        _ => "macos",
    }
    .to_string()
}

fn get_zig_target(target: TargetPlatform) -> String {
    match target {
        TargetPlatform::Linux => "x86_64-unknown-linux-gnu.2.17",
        TargetPlatform::Windows => "x86_64-pc-windows-msvc",
        TargetPlatform::MacosAarch64 => "aarch64-apple-darwin",
        TargetPlatform::MacosX86_64 => "x86_64-apple-darwin",
    }
    .to_string()
}

fn get_rust_target(target: TargetPlatform) -> String {
    match target {
        TargetPlatform::Linux => "x86_64-unknown-linux-gnu",
        TargetPlatform::Windows => "x86_64-pc-windows-msvc",
        TargetPlatform::MacosAarch64 => "aarch64-apple-darwin",
        TargetPlatform::MacosX86_64 => "x86_64-apple-darwin",
    }
    .to_string()
}

async fn compile_native(version: &str, target: TargetPlatform) -> Result<(), anyhow::Error> {
    let sources_directory = nuke_source_directory(version);
    let crates_path = path_to_string(
        &crate_root()
            .join("crates")
            .join("opendefocus-nuke")
            .join("Cargo.toml"),
    )?;

    if target == TargetPlatform::Windows
        && !nuke_source_directory(version).join("DDImage.lib").exists()
    {
        create_lib_from_dll(&nuke_source_directory(version).join("DDImage.dll")).await?;
    }

    let mut expression = cmd!(
        "cargo",
        "build",
        "--manifest-path",
        &crates_path,
        "--release",
    )
    .env("CPP_VERSION", format!("{}", get_cpp_version(version)?))
    .env("NUKE_SOURCE_PATH", &sources_directory)
    .env("PLATFORM_NAME", get_platform_name(target));
    if use_cxx11_abi(version)? {
        expression = expression.env("USE_CXX_ABI", "1");
    };

    expression.run()?;

    let dylib = dll_suffix(target);
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!("OpenDefocus.{}", dll_suffix(target)));
    let build_lib = path_to_string(
        &target_directory()
            .join("release")
            .join(format!("{}opendefocus_nuke.{dylib}", dll_prefix(target))),
    )?;
    tokio::fs::rename(build_lib, output_dylib).await?;
    Ok(())
}

async fn compile_zig(version: &str, target: TargetPlatform) -> Result<(), anyhow::Error> {
    let sources_directory = nuke_source_directory(version);

    let crates_path = path_to_string(
        &crate_root()
            .join("crates")
            .join("opendefocus-nuke")
            .join("Cargo.toml"),
    )?;
    let mut expression = cmd!(
        "cargo",
        "zigbuild",
        "--manifest-path",
        &crates_path,
        "--release",
        "--target",
        get_zig_target(target)
    )
    .env("CPP_VERSION", format!("{}", get_cpp_version(version)?))
    .env("NUKE_SOURCE_PATH", &sources_directory)
    .env("PLATFORM_NAME", get_platform_name(target))
    .env("USING_ZIG", "1");

    if use_cxx11_abi(version)? {
        expression = expression.env("USE_CXX_ABI", "1");
    };

    expression.run()?;

    let dylib = dll_suffix(target);
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!("OpenDefocus.{}", dll_suffix(target)));
    let build_lib = path_to_string(
        &target_directory()
            .join(get_rust_target(target))
            .join("release")
            .join(format!("{}opendefocus_nuke.{dylib}", dll_prefix(target))),
    )?;
    tokio::fs::rename(build_lib, output_dylib).await?;
    Ok(())
}

pub async fn create_lib_from_dll(dll: &Path) -> Result<()> {
    // just quick and dirty
    let dumpbin = find_msvc_tools::find("x86_64", "dumpbin.exe").unwrap();
    let exports = cmd!(dumpbin.get_program(), "/EXPORTS", dll)
        .dir(dll.parent().unwrap())
        .read()?;
    let mut lines = vec!["EXPORTS\n".to_string()];
    for (i, line) in exports.lines().enumerate() {
        if i < 19 {
            continue;
        }
        if line.len() > 26 {
            lines.push(line[26..].to_string());
            lines.push("\n".to_string());
        }
    }

    let exports = lines.iter().cloned().collect::<String>();

    let def = PathBuf::from("DDImage.def");
    tokio::fs::write(dll.parent().unwrap().join(&def), exports).await?;

    let lib = find_msvc_tools::find("x86_64", "lib.exe").unwrap();
    cmd!(
        lib.get_program(),
        "/def:DDImage.def",
        "/machine:x64",
        "/out:DDImage.lib"
    )
    .dir(dll.parent().unwrap())
    .run()?;

    Ok(())
}
