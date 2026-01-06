# Crates

The project has multiple crates defined at the crates directory, each of them serve their own purpose:

- [opendefocus](./opendefocus.md): The actual public library itself. The `main` crate.
- [opendefocus-datastructure](./opendefocus_datastructure.md): Datastructure bindings to protobuf and implementations.
- [opendefocus-kernel](./opendefocus_kernel.md): Kernel (`no-std`) source code. Runs on both GPU and CPU.
- [opendefocus-nuke](./opendefocus_nuke.md): Nuke specific source code. Includes both C++ and Rust.
- [opendefocus-shared](./opendefocus_shared.md): Code that can be used by both the [kernel](./opendefocus_kernel.md) and main [opendefocus](./opendefocus.md) crate.
