"""Plugin loader and lookup script."""

# ruff: noqa: ANN202, UP032, PTH118, PTH120,PTH112

import logging
import os
import platform

import nuke  # ty:ignore[unresolved-import]

from opendefocus_plugin._consts import INSTALLATION_PATH

logger = logging.getLogger(__name__)

NUKE_ARM_VERSION = 15
"""First version that has ARM support."""


class PluginNotFoundError(Exception):
    """Exception to raise when the plugin path is not found."""


class UnsupportedSystemError(Exception):
    """Exception to raise when the operating system is not supported."""


def _get_nuke_version():
    # type: () -> None
    """Return the Nuke version in Major.Minor."""
    return "{}.{}".format(nuke.NUKE_VERSION_MAJOR, nuke.NUKE_VERSION_MINOR)


def _get_operating_system_name():
    # type: () -> str
    """Return the operating system name to match the folder names.

    Raises:
        UnsupportedSystemError: when the current system does
                                not match anything.
    """
    operating_system = platform.system()
    operating_system = operating_system.lower()

    if "linux" in operating_system:
        return "linux"
    if "windows" in operating_system:
        return "windows"
    if "darwin" in operating_system:
        return "macos"

    msg = "System '{}' is not supported.".format(operating_system)
    raise UnsupportedSystemError(msg)


def _get_arch():
    # type: () -> str
    """Return the architecture shortname for the current system.

    Note:
        On arm64 macOS systems, which are using Nuke 14 <= lower,
        it will return x86_64 as it uses Rosetta.

    Raises:
        UnsupportedSystemError: when the current architecture is not supported.
    """
    architecture = platform.processor()

    if architecture in ("i386", "x86_64") or platform.system().lower() != "darwin":
        return "x86_64"
    if "arm" in architecture and nuke.NUKE_VERSION_MAJOR >= NUKE_ARM_VERSION:
        return "aarch64"
    if "arm" in architecture:
        return "x86_64"

    msg = "Architecture '{}' is not supported.".format(architecture)
    raise UnsupportedSystemError(msg)


def _build_plugin_path():
    # type: () -> str
    """Build the expected plugin path.

    For example `my_nuke_install/opendefocus/bin/15.0/linux`.

    Returns:
        plugin path where plugin should be located.
    """
    return os.path.join(
        INSTALLATION_PATH,
        "bin",
        _get_nuke_version(),
        _get_operating_system_name(),
        _get_arch(),
    ).replace(os.sep, "/")


def _build_plugin_path_dev():
    # type: () -> str
    """Build the expected plugin path during development.

    Returns:
        plugin path where plugin should be located.
    """
    return os.path.join(
        os.path.dirname(os.path.dirname(os.path.dirname(os.path.dirname(__file__)))),
        "build",
        "lib",
    ).replace(os.sep, "/")


def add_plugin_path():
    # type: () -> str
    """Add the plugin path to Nuke if found.

    Raises:
        PluginNotFoundError: if the plugin path is not found and could not be loaded.
    """
    os.environ["OPENDEFOCUS_LOADED"] = "0"

    plugin_path = _build_plugin_path()
    if not os.path.isdir(plugin_path):
        msg = (
            "OpenDefocus is installed, "
            "however this version of Nuke: '{}' "
            "is not supported in this release. "
            "Please make sure to update to the latest version "
            "of OpenDefocus.".format(nuke.NUKE_VERSION_STRING)
        )
        raise PluginNotFoundError(msg)

    nuke.pluginAppendPath(str(plugin_path))  # ty:ignore[unresolved-attribute]
    os.environ["OPENDEFOCUS_LOADED"] = "1"


def add_plugin_path_safe():
    # type: () -> None
    """Add the plugin path to Nuke if found, else send to the logger."""
    try:
        add_plugin_path()
        logger.info("OpenDefocus is loaded successfully.")

    except PluginNotFoundError:
        logger.exception("Plugin loading failed.")
