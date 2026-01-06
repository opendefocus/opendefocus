# Building

Building is really easy, everything is handled with the included [xtask](./xtask.md).

You need to have Rust installed:
Both the stable version (1.92+) as well as the nightly version listed in [spirv-cli-build/rust-toolchain.toml](./crates/spirv-cli-build/rust-toolchain.toml)

That's it basically.

All compilation is handled through xtasks. Call `cargo xtask --help` for more information.

### Nuke
For Nuke building you need additional dependencies. As the linking process needs to have the DDImage library and headers installed, the xtask fetches these sources automatically.

For extracting of the archives, `msiextract` needs to be installed for Windows installs.

In theory C++11 is supported so it would work with Nuke 10 and higher, but that is not tested.

When compiling Nuke and fetching sources, to speed up downloading it is recommended to either use a machine in the USA or use a VPN. Because its an AWS bucket in the US-East region which does not have geo-replication enabled.

Example to compile Nuke 15:
```bash
cargo xtask \
  --compile \
  --nuke-versions 15.1,15.2 \
  --target-platform linux \
  --output-to-package
```

This will create the package with Linux compiled binaries in the [package](./package/).

Now you are able to copy this folder to some other location, or add it to your `NUKE_PATH` and it should show up.

```bash
export NUKE_PATH=$(pwd)/package

# launch your nuke etc...
```
