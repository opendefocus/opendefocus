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

pub async fn compile_nuke(versions: Vec<String>, target: TargetPlatform, limit_threads: bool) -> Result<()> {
    get_sources(vec![target], versions.clone(), limit_threads).await?;
    let xwin_path = target_directory().join("xwin");
    if target == TargetPlatform::Windows && !xwin_path.exists() {
        cmd!("xwin", "--accept-license", "splat", "--output", xwin_path).run()?;
    };

    for version in versions {
        let cpp_version = get_cpp_version(&version)?;
        println!("cargo:rustc-env=CPP_VERSION={}", cpp_version);
        if target == TargetPlatform::MacosAarch64 || target == TargetPlatform::MacosX86_64 {
            if std::env::consts::ARCH == "aarch64"
                && target == TargetPlatform::MacosAarch64
                && std::env::consts::OS == "macos"
            {
                compile_macos_native(&version, &target).await?
            } else {
                compile_macos_zig(&version, &target).await?
            }
        } else if target == TargetPlatform::Linux {
            compile_linux_zig(&version, &target).await?
        } else if target == TargetPlatform::Windows {
            todo!()
        }
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

async fn compile_macos_native(version: &str, target: &TargetPlatform) -> Result<(), anyhow::Error> {
    unsafe {
        std::env::set_var("MACOSX_DEPLOYMENT_TARGET", "11.0");
    }
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
    let staticlib = static_file_extension(target);
    let src = crate_root()
        .join("crates")
        .join("opendefocus-nuke")
        .join("src")
        .join("opendefocus.cpp");
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!(
        "OpenDefocus.{}",
        dll_suffix(TargetPlatform::MacosAarch64)
    ));
    let build_staticlib = path_to_string(&&target_directory().join("release").join(format!(
        "{}opendefocus_nuke.{staticlib}",
        dll_prefix(TargetPlatform::MacosAarch64)
    )))?;
    cmd!(
        "clang++",
        "-c",
        "-Wno-ignored-qualifiers",
        format!("-I{}", path_to_string(&sources_directory.join("include"))?),
        format!("-I{}crates", path_to_string(&crate_root())?),
        format!("-I{}cxxbridge", path_to_string(&target_directory())?),
        format!("-std=c++{}", get_cpp_version(version)?),
        "-fPIC",
        "-o",
        &out_dir.join(format!(
            "{}opendefocus.o",
            dll_prefix(TargetPlatform::MacosAarch64)
        )),
        src,
    )
    .run()?;
    cmd!(
        "clang++",
        format!("-L{}", path_to_string(&sources_directory)?),
        "-lDDImage",
        "-framework",
        "Foundation",
        "-framework",
        "Metal",
        "-framework",
        "MetalKit",
        "-shared",
        "-o",
        output_dylib,
        &out_dir.join(format!(
            "{}opendefocus.o",
            dll_prefix(TargetPlatform::MacosAarch64)
        )),
        build_staticlib,
    )
    .run()?;
    Ok(())
}

async fn compile_linux_zig(version: &str, target: &TargetPlatform) -> Result<(), anyhow::Error> {
    let glibc = "2.17";
    let zigbuild_target = "x86_64-unknown-linux-gnu";
    let sources_directory = nuke_source_directory(version);
    let crates_path = path_to_string(
        &crate_root()
            .join("crates")
            .join("opendefocus-nuke")
            .join("Cargo.toml"),
    )?;
    cmd!(
        "cargo",
        "zigbuild",
        "--manifest-path",
        &crates_path,
        "--release",
        "--target",
        format!("{zigbuild_target}.{glibc}"),
    )
    .env("NUKE_SOURCE_PATH", &sources_directory)
    .run()?;
    println!(
        "cargo:rustc-env=NUKE_SOURCE_PATH={}",
        path_to_string(&sources_directory)?
    );
    let staticlib = static_file_extension(target);
    let src = crate_root()
        .join("crates")
        .join("opendefocus-nuke")
        .join("src")
        .join("opendefocus.cpp");
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!("OpenDefocus.{}", dll_suffix(*target)));
    let build_staticlib = path_to_string(
        &target_directory()
            .join(zigbuild_target)
            .join("release")
            .join(format!(
                "{}opendefocus_nuke.{staticlib}",
                dll_prefix(*target)
            )),
    )?;
    cmd!(
        "g++",
        "-c",
        "-Wno-ignored-qualifiers",
        "-DGLEW_NO_GLU",
        "-Wno-date-time",
        "-Wno-unused-parameter",
        "-D_GLIBCXX_USE_CXX11_ABI=1",
        format!("-I{}", path_to_string(&sources_directory.join("include"))?),
        format!("-I{}crates", path_to_string(&crate_root())?),
        format!(
            "-I{}{zigbuild_target}/cxxbridge",
            path_to_string(&target_directory())?
        ),
        format!("-std=c++{}", get_cpp_version(version)?),
        "-fPIC",
        "-o",
        &out_dir.join(format!(
            "{}opendefocus.o",
            dll_prefix(TargetPlatform::Linux)
        )),
        src,
    )
    .run()?;
    cmd!(
        "g++",
        format!("-L{}", path_to_string(&sources_directory)?),
        "-lDDImage",
        "-D_GLIBCXX_USE_CXX11_ABI=1",
        "-shared",
        "-o",
        output_dylib,
        &out_dir.join(format!("{}opendefocus.o", dll_prefix(*target))),
        build_staticlib,
    )
    .run()?;
    Ok(())
}

async fn compile_macos_zig(version: &str, target: &TargetPlatform) -> Result<(), anyhow::Error> {
    unsafe {
        std::env::set_var("MACOSX_DEPLOYMENT_TARGET", "11.0");
    }
    let (zigbuild_target, zig_target) = match target {
        TargetPlatform::MacosAarch64 => ("aarch64-apple_darwin", "aarch64-macos"),
        _ => ("x86_64-apple_darwin", "x86_64-macos"),
    };
    let sources_directory = nuke_source_directory(version);
    let crates_path = path_to_string(
        &crate_root()
            .join("crates")
            .join("opendefocus-nuke")
            .join("Cargo.toml"),
    )?;
    cmd!(
        "cargo",
        "zigbuild",
        "--manifest-path",
        &crates_path,
        "--release",
        "--target",
        zigbuild_target,
    )
    .env("NUKE_SOURCE_PATH", &sources_directory)
    .run()?;
    println!(
        "cargo:rustc-env=NUKE_SOURCE_PATH={}",
        path_to_string(&sources_directory)?
    );
    let staticlib = static_file_extension(target);
    let src = crate_root()
        .join("crates")
        .join("opendefocus-nuke")
        .join("src")
        .join("opendefocus.cpp");
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!(
        "OpenDefocus.{}",
        dll_suffix(TargetPlatform::MacosAarch64)
    ));
    let build_staticlib = path_to_string(
        &target_directory()
            .join(zigbuild_target)
            .join("release")
            .join(format!(
                "{}opendefocus_nuke.{staticlib}",
                dll_prefix(TargetPlatform::MacosAarch64)
            )),
    )?;
    cmd!(
        "zig",
        "c++",
        "-target",
        zig_target,
        "-c",
        "-Wno-ignored-qualifiers",
        format!("-I{}", path_to_string(&sources_directory.join("include"))?),
        format!("-I{}crates", path_to_string(&crate_root())?),
        format!(
            "-I{}{zigbuild_target}/cxxbridge",
            path_to_string(&target_directory())?
        ),
        format!("-std=c++{}", get_cpp_version(version)?),
        "-fPIC",
        "-o",
        &out_dir.join(format!(
            "{}opendefocus.o",
            dll_prefix(TargetPlatform::MacosAarch64)
        )),
        src,
    )
    .run()?;
    cmd!(
        "zig",
        "c++",
        "-target",
        zig_target,
        format!("-L{}", path_to_string(&sources_directory)?),
        "--sysroot",
        std::env::var("SDKROOT").unwrap_or_default(),
        format!(
            "-F{}",
            path_to_string(
                &PathBuf::from(std::env::var("SDKROOT").unwrap_or_default())
                    .join("System")
                    .join("Library")
                    .join("Frameworks")
            )?
        ),
        "-lDDImage",
        "-framework",
        "Foundation",
        "-framework",
        "Metal",
        "-framework",
        "MetalKit",
        "-shared",
        "-o",
        output_dylib,
        &out_dir.join(format!(
            "{}opendefocus.o",
            dll_prefix(TargetPlatform::MacosAarch64)
        )),
        build_staticlib,
    )
    .run()?;
    Ok(())
}

fn static_file_extension(target: &TargetPlatform) -> String {
    match target {
        TargetPlatform::Windows => "lib",
        _ => "a",
    }
    .to_string()
}
