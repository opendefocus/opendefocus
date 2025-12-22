use crate::{
    TargetPlatform,
    nuke::sources::dll_suffix,
    util::{crate_root, target_directory},
};
use anyhow::Result;

pub async fn create_package(target: TargetPlatform, versions: Vec<String>) -> Result<()> {
    let target_path = crate_root().join("package");

    for version in &versions {
        let (os_name, arch_name) = match target {
            TargetPlatform::Linux => ("linux", "x86_64"),
            TargetPlatform::Windows => ("windows", "x86_64"),
            TargetPlatform::MacosAarch64 => ("macos", "aarch64"),
            TargetPlatform::MacosX86_64 => ("macos", "x86_64"),
        };

        let target_binary_path = target_path
            .join("opendefocus")
            .join("bin")
            .join(version)
            .join(os_name)
            .join(arch_name);
        tokio::fs::create_dir_all(&target_binary_path).await?;
        let filename = format!("OpenDefocus.{}", dll_suffix(target));
        let source_binary_path = target_directory()
            .join("nuke")
            .join(version)
            .join(format!("{arch_name}-{os_name}"))
            .join(&filename);

        tokio::fs::rename(source_binary_path, target_binary_path.join(filename)).await?;
    }
    Ok(())
}
