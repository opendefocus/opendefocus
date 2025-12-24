use cxx_build::CFG;
use miette::Result;
use std::path::PathBuf;

fn main() -> Result<()> {
    let nuke_path = if let Ok(sources) = std::env::var("NUKE_SOURCE_PATH") {
        PathBuf::from(sources).join("include")
    } else {
        return Ok(());
    };

    let cpp_version = std::env::var("CPP_VERSION").unwrap_or("17".to_string());
    CFG.exported_header_dirs.extend([nuke_path.as_path()]);

    cxx_build::bridge("src/lib.rs")
        .std(&format!("c++{cpp_version}"))
        .define("__gnu_cxx", "std")
        .define("_GLIBCXX_USE_CXX11_ABI", "1")
        .flag("-fPIC")
        // .cpp_link_stdlib("stdc++")
        // .flag_if_supported("-stdlib=libstdc++")
        .flag_if_supported("-DGLEW_NO_GLU")
        // .flag_if_supported("-fvisibility=hidden")
        .flag_if_supported("-Wno-deprecated-copy-with-user-provided-copy")
        .flag_if_supported("-Wno-ignored-qualifiers")
        .flag_if_supported("-Wno-date-time")
        .flag_if_supported("-Wno-unused-parameter") // as a lot of stuff is produced because of third-party headers :)
        .file("src/bridge.cpp")
        .file("src/opendefocus.cpp")
        .cpp(true)
        .compile("opendefocus-nuke");

    println!("cargo:rerun-if-changed=include/opendefocus.hpp");
    println!("cargo:rerun-if-changed=include/bridge.hpp");
    println!("cargo:rerun-if-changed=src/opendefocus.cpp");
    println!("cargo:rerun-if-changed=src/bridge.cpp");
    println!("cargo:rustc-link-search=all={}", std::env::var("NUKE_SOURCE_PATH").unwrap());
    println!("cargo:rustc-link-lib=dylib=DDImage");

    // #[cfg(target_os = "macos")]
    // {
    //     println!("cargo:rustc-link-lib=framework=Foundation");
    //     println!("cargo:rustc-link-lib=framework=Metal");
    // }


    Ok(())
}
