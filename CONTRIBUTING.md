# Contributing to OpenDefocus

Contributing to OpenDefocus is greatly appreciated and welcome! To start, I'd recommend to look at the open issues. Or create one yourself and start on that if you have some suggestions for improvements.

Once done, please create a PR to the `main` branch and we can discuss (and hopefully) merge it.

## Development

### Tools Required

- [Rust](https://rustup.rs/)
- Protoc ([protobuf-compiler on Linux](https://packages.debian.org/sid/protobuf-compiler)/[protobuf on macOS](https://formulae.brew.sh/formula/protobuf)/[protoc on Windows](https://community.chocolatey.org/packages/protoc))
- [Codeberg](https://codeberg.org/user/sign_up) account

### If you want to do some Nuke stuff

#### Linux
- [Zig](https://zig.guide/getting-started/installation/) for C++ builds with glibc 2.17
- [cargo-zigbuild](https://github.com/rust-cross/cargo-zigbuild) same but for Rust

#### macOS
- [Command line tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools) for C++ builds

#### Windows
- [MSVC build tools 2022](https://visualstudio.microsoft.com/downloads/) for C++ builds
- [lessmsi](https://community.chocolatey.org/packages/lessmsi) for extraction of Nuke installer


If you can't figure it out, feel free to create an issue!

### Getting Started

1. Fork the repository to your **Codeberg** account.

   ```bash
   git clone ssh://git@codeberg.org/YOUR_USERNAME/opendefocus.git
   cd opendefocus/
   ```
2. Create a branch with a descriptive name.
   - It is recommended to give your branch a meaningful name, relevant to the feature or fix you are working on.
     - Good examples:
       - `docs-plugin-installation`
       - `feature-new-nonuniform-artifact`
       - `fix-panic-during-this-operation`

3. Commit using the [conventional commits specifications](https://www.conventionalcommits.org/en/v1.0.0/)

3. Run the development environment. Everything is handled through the xtask.
    - To build for Nuke for example with Zig on Linuz, use `cargo xtask --compile --nuke-versions 15.0 --target-platform linux --use-zig --output-to-package`
    - If you can't figure it out, take a look at [release_nuke.yaml](https://codeberg.org/opendefocus/opendefocus/src/branch/main/.github/workflows/release_nuke.yaml) how builds are done.

4. Create your contribution and test the changes.
    - If you want to open the changes in Nuke, set the `NUKE_PATH` to the package directory in this cloned local repository.

---

## Most of all
Thank you!! For taking the time to read this and to contribute your time into this project.