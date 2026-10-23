# <i class="fa-solid fa-wrench"></i> Installation

Download the installer for your system from the [latest release](https://github.com/opendefocus/opendefocus/releases/latest) on GitHub, then **restart your host**. A host that is already running does not see a new plugin.

| System | Download | Installs to |
| --- | --- | --- |
| Linux | `OpenDefocus-OFX-<version>-linux-<arch>.tar.gz` | `/usr/OFX/Plugins` |

`<arch>` is `x86_64` or `aarch64`.

Extract the archive and run `./install.sh` (use `--prefix DIR` for another location). The folder also contains `uninstall.sh`.

Releases are published for Linux only. On macOS and Windows, build the plugin from source with the [building guide](./developer/building.md).

Each download has a matching `.sha256` file to verify it.

> [!IMPORTANT]
> The plugin renders on the GPU (through WGPU). The machine needs a GPU, or on Linux a software Vulkan driver such as Mesa lavapipe.

See [Natron and Resolve](./hosts.md) for using the plugin in a host. To build from source, including the CLI and viewer, see the [building guide](./developer/building.md).
