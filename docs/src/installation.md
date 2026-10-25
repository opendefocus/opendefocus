# <i class="fa-solid fa-wrench"></i> Installation

Download the installer for your system from the [latest release](https://github.com/opendefocus/opendefocus/releases/latest) on GitHub, then **restart your host**. A host that is already running does not see a new plugin.

| System | Download | Installs to |
| --- | --- | --- |
| Linux | `OpenDefocus-OFX-<version>-linux-<arch>.tar.gz` | `/usr/OFX/Plugins` |
| Windows | `OpenDefocus-OFX-<version>-windows-x86_64-setup.exe` | `%CommonProgramFiles%\OFX\Plugins` |

`<arch>` is `x86_64` or `aarch64` on Linux.

**Linux:** extract the archive and run `./install.sh` (use `--prefix DIR` for another location). The folder also contains `uninstall.sh`.

**Windows:** run the setup `.exe`. The installer is not signed yet, so Windows SmartScreen shows "Windows protected your PC". Click **More info**, then **Run anyway**. Uninstall it from **Settings > Apps**.

Releases are published for Linux and Windows. On macOS, build the plugin from source with the [building guide](./developer/building.md).

Each download has a matching `.sha256` file to verify it.

> [!IMPORTANT]
> The plugin renders on the GPU (through WGPU). The machine needs a GPU, or on Linux a software Vulkan driver such as Mesa lavapipe.

See [Natron and Resolve](./hosts.md) for using the plugin in a host. To build from source, including the CLI and viewer, see the [building guide](./developer/building.md).
