use std::io::Result;

use cargo_metadata::MetadataCommand;
fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=NULL");
    build_proto()?;
    Ok(())
}

fn build_proto() -> Result<()> {
    #[cfg(feature = "compile-protobuf-src")]
    std::env::set_var("PROTOC", protobuf_src::protoc());

    let metadata = MetadataCommand::new().exec().unwrap();
    let circle_of_confusion_package = metadata
        .packages
        .iter()
        .find(|p| p.name == "circle-of-confusion")
        .unwrap();
    let bokeh_creator_package = metadata
        .packages
        .iter()
        .find(|p| p.name == "bokeh-creator")
        .unwrap();
    let mut config = prost_build::Config::new();
    #[cfg(feature = "serde")]
    config.type_attribute(".", "#[derive(serde::Serialize,serde::Deserialize)]");
    #[cfg(feature = "documented")]
    config.type_attribute(".", "#[derive(documented::DocumentedFields)]");
    // config.field_attribute(".", attribute)
    config.extern_path(".circle_of_confusion", "circle_of_confusion");
    config.extern_path(".bokeh_creator", "bokeh_creator");
    // config.extern_path(".bokeh_creator", "::bokeh-creator")
    let includes = [
        "../../proto".to_string(),
        circle_of_confusion_package
            .manifest_path
            .parent()
            .unwrap()
            .join("proto")
            .to_string(),
        bokeh_creator_package
            .manifest_path
            .parent()
            .unwrap()
            .join("proto")
            .to_string(),
    ];
    config.compile_protos(&["opendefocus.proto"], &includes)?;

    Ok(())
}
