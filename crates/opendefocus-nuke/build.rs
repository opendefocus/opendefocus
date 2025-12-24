use cxx_build::CFG;
use miette::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let nuke_path = if let Ok(sources) = std::env::var("NUKE_SOURCE_PATH") {
        PathBuf::from(sources).join("include")
    } else {
        return Ok(());
    };
    let platform_name = if let Ok(name) = std::env::var("PLATFORM_NAME") {
        name
    } else {
        return Ok(());
    };

    let cpp_version = std::env::var("CPP_VERSION").unwrap_or("17".to_string());
    CFG.exported_header_dirs.extend([nuke_path.as_path()]);

    let mut builder = cxx_build::bridge("src/lib.rs");
    builder
        .std(&format!("c++{cpp_version}"))
        .flag_if_supported("-DGLEW_NO_GLU")
        .file("src/opendefocus.cpp")
        .cpp(true);

    if platform_name == "linux" {
        builder
            .flag("-fPIC")
            .cpp_link_stdlib("stdc++")
            .flag_if_supported("-Wno-deprecated-copy-with-user-provided-copy")
            .flag_if_supported("-Wno-ignored-qualifiers")
            .flag_if_supported("-Wno-date-time")
            .flag_if_supported("-Wno-unused-parameter")
            .flag_if_supported("-DGLEW_NO_GLU");
        if std::env::var("USE_CXX11_ABI").is_ok() {
            builder.flag("-D_GLIBCXX_USE_CXX11_ABI");
        };
        if std::env::var("USING_ZIG").is_ok() {
            builder.define("__gnu_cxx", "std");
        };
    } else if platform_name == "macos" {
        builder
            .flag_if_supported("-Wno-deprecated-copy-with-user-provided-copy")
            .flag_if_supported("-Wno-ignored-qualifiers")
            .flag_if_supported("-Wno-date-time")
            .flag_if_supported("-Wno-unused-parameter");
    } else if platform_name == "windows" {
    }
    builder.compile("opendefocus-nuke");

    println!("cargo:rerun-if-changed=include/opendefocus.hpp");
    println!("cargo:rerun-if-changed=src/opendefocus.cpp");
    println!(
        "cargo:rustc-link-search=all={}",
        std::env::var("NUKE_SOURCE_PATH").unwrap()
    );
    println!("cargo:rustc-link-lib=dylib=DDImage");

    Ok(())
}
