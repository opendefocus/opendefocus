# opendefocus-datastructure

This crate both generates the bindings using the Protobuf definition, and has some additional implementations to the structs.

## Why protobuf?
To make it easy to add additional language support as Protobuf can be send over bytes. So this means we can encode and decode settings from any language.


## Structure
[opendefocus.proto](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-datastructure/proto/opendefocus.proto): definition of opendefocus settings
[build.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-datastructure/build.rs): includes the building process for converting protobuf to Rust structs
[lib.rs](https://codeberg.org/opendefocus/opendefocus/src/branch/main/crates/opendefocus-datastructure/src/lib.rs): bindings to the protobuf structs and additional implementations which are often used on the structs
