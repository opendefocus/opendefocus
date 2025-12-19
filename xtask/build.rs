use anyhow::Result;
fn main() -> Result<()> {
    println!("cargo:rustc-env=HOST={}", std::env::var("HOST")?);
    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET")?);
    println!(
        "cargo::rustc-env=BUILD_DIR={}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../")
            .join("target")
            .to_str()
            .unwrap()
    );
    println!(
        "cargo::rustc-env=WORKSPACE={}",
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../")
            .to_str()
            .unwrap()
    );

    Ok(())
}
