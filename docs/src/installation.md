# <i class="fa-solid fa-wrench"></i> Installation

Download the installer for your system from the [latest release](https://github.com/opendefocus/opendefocus/releases/latest) on GitHub, then **restart your host**. A host that is already running does not see a new plugin.

| System | Download | Installs to |
| --- | --- | --- |
| macOS | `OpenDefocus-OFX-<version>-macos-<arch>.pkg` | `/Library/OFX/Plugins` |
| Windows | `OpenDefocus-OFX-<version>-windows-x86_64-setup.exe` | `%CommonProgramFiles%\OFX\Plugins` |
| Linux | `OpenDefocus-OFX-<version>-linux-<arch>.tar.gz` | `/usr/OFX/Plugins` |

`<arch>` is `arm64` or `x86_64` on macOS, and `x86_64` or `aarch64` on Linux.

- **macOS and Windows:** run the installer. The installers may not be signed yet, so your system can warn before opening them. On macOS, right-click the `.pkg` and choose **Open**.
- **Linux:** extract the archive and run `./install.sh` (use `--prefix DIR` for another location). The folder also contains `uninstall.sh`.

Each download has a matching `.sha256` file to verify it.

> [!IMPORTANT]
> The plugin renders on the GPU (through WGPU). The machine needs a GPU, or on Linux a software Vulkan driver such as Mesa lavapipe.

See [Natron and Resolve](./hosts.md) for using the plugin in a host. To build from source, including the CLI and viewer, see the [building guide](./developer/building.md).
