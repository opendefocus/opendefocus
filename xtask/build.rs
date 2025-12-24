use anyhow::Result;
fn main() -> Result<()> {
    println!("cargo:rustc-env=HOST={}", std::env::var("HOST")?);
    println!("cargo:rustc-env=TARGET={}", std::env::var("TARGET")?);
    let target_dir = if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
        target_dir.to_owned()
    } else {
        format!("{}/../target/", env!("CARGO_MANIFEST_DIR"))
    };

    println!("cargo:rustc-env=TARGET_DIRECTORY={}", target_dir);
    Ok(())
}
