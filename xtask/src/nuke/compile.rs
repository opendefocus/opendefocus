use core::panic;
use std::path::PathBuf;

use anyhow::Result;
use duct::cmd;

use crate::{
    TargetPlatform,
    nuke::sources::{dll_prefix, dll_suffix, get_sources, nuke_source_directory},
};

fn build_dir(version: &str, target: &TargetPlatform) -> PathBuf {
    PathBuf::from(env!("BUILD_DIR"))
        .join("nuke")
        .join(version)
        .join(format!("{}", target))
}

pub async fn compile_nuke(versions: Vec<String>, targets: Vec<TargetPlatform>) -> Result<()> {
    get_sources(targets.clone(), versions.clone()).await?;
    let xwin_path = PathBuf::from(env!("WORKSPACE")).join("target").join("xwin");
    if targets.contains(&TargetPlatform::Windows) && !xwin_path.exists() {
        cmd!("xwin", "--accept-license", "splat", "--output", xwin_path).run()?;
    };

    for version in versions {
        let cpp_version = get_cpp_version(&version)?;
        println!("cargo:rustc-env=CPP_VERSION={}", cpp_version);
        for target in &targets {
            if target == &TargetPlatform::MacosAarch64 || target == &TargetPlatform::MacosX86_64 {
                if std::env::consts::ARCH == "aarch64"
                    && target == &TargetPlatform::MacosAarch64
                    && std::env::consts::OS == "macos"
                {
                    compile_macos_native(&version, target).await?
                } else {
                    compile_macos_zig(&version, &target).await?
                }
            } else if target == &TargetPlatform::Linux {
                compile_linux_zig(&version, &target).await?
            } else if target == &TargetPlatform::Windows {
                todo!()
            }
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
    let crates_path = PathBuf::from(env!("WORKSPACE"))
        .join("crates")
        .join("opendefocus-nuke")
        .join("Cargo.toml")
        .to_str()
        .unwrap()
        .to_owned();
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
        sources_directory.to_str().unwrap()
    );
    let staticlib = static_file_extension(target);
    let src = PathBuf::from(env!("WORKSPACE"))
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
    let build_staticlib = PathBuf::from(env!("WORKSPACE"))
        .join("target")
        .join("release")
        .join(format!(
            "{}opendefocus_nuke.{staticlib}",
            dll_prefix(TargetPlatform::MacosAarch64)
        ))
        .to_str()
        .unwrap()
        .to_string();
    cmd!(
        "clang++",
        "-c",
        "-Wno-ignored-qualifiers",
        format!("-I{}", sources_directory.join("include").to_str().unwrap()),
        format!(
            "-I{}",
            sources_directory
                .parent()
                .unwrap()
                .join("include")
                .to_str()
                .unwrap()
        ),
        format!("-I/{}crates", env!("WORKSPACE")),
        format!("-I/{}target/cxxbridge", env!("WORKSPACE")),
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
        format!("-L{}", sources_directory.to_str().unwrap()),
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
    let crates_path = PathBuf::from(env!("WORKSPACE"))
        .join("crates")
        .join("opendefocus-nuke")
        .join("Cargo.toml")
        .to_str()
        .unwrap()
        .to_owned();
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
        sources_directory.to_str().unwrap()
    );
    let staticlib = static_file_extension(target);
    let src = PathBuf::from(env!("WORKSPACE"))
        .join("crates")
        .join("opendefocus-nuke")
        .join("src")
        .join("opendefocus.cpp");
    let out_dir = build_dir(version, target);
    if !out_dir.is_dir() {
        tokio::fs::create_dir_all(&out_dir).await?;
    }
    let output_dylib = out_dir.join(format!("OpenDefocus.{}", dll_suffix(*target)));
    let build_staticlib = PathBuf::from(env!("WORKSPACE"))
        .join("target")
        .join(zigbuild_target)
        .join("release")
        .join(format!(
            "{}opendefocus_nuke.{staticlib}",
            dll_prefix(*target)
        ))
        .to_str()
        .unwrap()
        .to_string();
    cmd!(
        "g++",
        "-c",
        "-Wno-ignored-qualifiers",
        "-DGLEW_NO_GLU",
        "-Wno-date-time",
        "-Wno-unused-parameter",
        "-D_GLIBCXX_USE_CXX11_ABI=1",
        format!("-I{}", sources_directory.join("include").to_str().unwrap()),
        format!("-I/{}crates", env!("WORKSPACE")),
        format!("-I/{}target/{zigbuild_target}/cxxbridge", env!("WORKSPACE")),
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
        format!("-L{}", sources_directory.to_str().unwrap()),
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
    let crates_path = PathBuf::from(env!("WORKSPACE"))
        .join("crates")
        .join("opendefocus-nuke")
        .join("Cargo.toml")
        .to_str()
        .unwrap()
        .to_owned();
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
        sources_directory.to_str().unwrap()
    );
    let staticlib = static_file_extension(target);
    let src = PathBuf::from(env!("WORKSPACE"))
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
    let build_staticlib = PathBuf::from(env!("WORKSPACE"))
        .join("target")
        .join(zigbuild_target)
        .join("release")
        .join(format!(
            "{}opendefocus_nuke.{staticlib}",
            dll_prefix(TargetPlatform::MacosAarch64)
        ))
        .to_str()
        .unwrap()
        .to_string();
    cmd!(
        "zig",
        "c++",
        "-target",
        zig_target,
        "-c",
        "-Wno-ignored-qualifiers",
        format!("-I{}", sources_directory.join("include").to_str().unwrap()),
        format!(
            "-I{}",
            sources_directory
                .parent()
                .unwrap()
                .join("include")
                .to_str()
                .unwrap()
        ),
        format!("-I/{}crates", env!("WORKSPACE")),
        format!("-I/{}target/{zigbuild_target}/cxxbridge", env!("WORKSPACE")),
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
        format!("-L{}", sources_directory.to_str().unwrap()),
        "--sysroot",
        std::env::var("SDKROOT").unwrap_or_default(),
        format!(
            "-F{}",
            PathBuf::from(std::env::var("SDKROOT").unwrap_or_default())
                .join("System")
                .join("Library")
                .join("Frameworks")
                .to_str()
                .unwrap()
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
