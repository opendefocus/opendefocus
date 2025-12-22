import os
import sys

try:
    from unittest.mock import MagicMock, patch
except ImportError:
    from unittest.mock import MagicMock, patch
import pytest

sys.modules["nuke"] = MagicMock()

from opendefocus_plugin._plugin_loader import (
    PluginNotFoundError,
    UnsupportedSystemError,
    _build_plugin_path,
    _get_arch,
    _get_nuke_version,
    _get_operating_system_name,
    add_plugin_path,
    add_plugin_path_safe,
)


@pytest.mark.parametrize(
    ("major", "minor", "expected"),
    [
        (12, 0, "12.0"),
        (12, 1, "12.1"),
        (15, 3, "15.3"),
    ],
)
def test_nuke_version_format(major, minor, expected):
    # type: (int, int, int) -> None
    """Test the nuke version to return an expected string."""
    nuke_mock = MagicMock()
    nuke_mock.NUKE_VERSION_MAJOR = major
    nuke_mock.NUKE_VERSION_MINOR = minor

    with patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock):
        version = _get_nuke_version()

    assert version == expected


@pytest.mark.parametrize(
    ("platform", "expected"),
    [
        ("Linux", "linux"),
        ("Windows", "windows"),
        ("Darwin", "macos"),
    ],
)
def test_get_operating_system_name(platform, expected):
    # type: (str, str) -> None
    """Test the operating system retrieval to match to the current system."""
    platform_mock = MagicMock()
    platform_mock.system.return_value = platform

    with patch("opendefocus_plugin._plugin_loader.platform", platform_mock):
        result = _get_operating_system_name()

    assert result == expected


def test_unsupported_system_error():
    """Test the operating system retrieval to match to the current system."""
    nuke_mock = MagicMock()
    nuke_mock.NUKE_VERSION_MAJOR = 15

    platform_mock = MagicMock()
    platform_mock.system.return_value = "SciFiOS"

    with patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock), patch(
        "opendefocus_plugin._plugin_loader.platform",
        platform_mock,
    ), pytest.raises(
        UnsupportedSystemError,
        match="System 'scifios' is not supported.",
    ):
        _get_operating_system_name()


@pytest.mark.parametrize(
    ("processor", "major", "system", "expected"),
    [
        ("i386", 15, "darwin", "x86_64"),
        ("arm", 15, "Darwin", "aarch64"),
        ("arm", 15, "darwin", "aarch64"),
        ("something not great", 15, "at least not darwin", "x86_64"),
    ],
)
def test_get_arch(processor, major, system, expected):
    # type: (str, str, str, str) -> None
    """Test arch retrieval to be one expected format"""
    nuke_mock = MagicMock()
    nuke_mock.NUKE_VERSION_MAJOR = major

    platform_mock = MagicMock()
    platform_mock.processor.return_value = processor
    platform_mock.system.return_value = system

    with patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock), patch(
        "opendefocus_plugin._plugin_loader.platform",
        platform_mock,
    ):
        result = _get_arch()
    assert result == expected


def test_unsupported_arch_system_error():
    """Test the operating system retrieval to match to the current system."""
    nuke_mock = MagicMock()
    nuke_mock.NUKE_VERSION_MAJOR = 15

    platform_mock = MagicMock()
    platform_mock.processor.return_value = "x28"
    platform_mock.system.return_value = "Darwin"

    with patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock), patch(
        "opendefocus_plugin._plugin_loader.platform", platform_mock
    ), pytest.raises(
        UnsupportedSystemError,
        match="Architecture 'x28' is not supported.",
    ):
        _get_arch()


def test_build_plugin_path():
    """Test the path to build from the installation path onwards."""
    test_installation_path = "test/installation/path"
    test_system = "test_system"
    test_arch = "test_arch"
    test_nuke_version = "15.0"
    expected_path = os.path.join(
        test_installation_path,
        "bin",
        test_nuke_version,
        test_system,
        test_arch,
    ).replace(os.sep, "/")

    with patch(
        "opendefocus_plugin._plugin_loader._get_operating_system_name",
        return_value=test_system,
    ), patch(
        "opendefocus_plugin._plugin_loader._get_nuke_version",
        return_value=test_nuke_version,
    ), patch(
        "opendefocus_plugin._plugin_loader._get_arch",
        return_value=test_arch,
    ), patch(
        "opendefocus_plugin._plugin_loader.INSTALLATION_PATH",
        test_installation_path,
    ):
        result = _build_plugin_path()

    assert result == expected_path


@pytest.mark.parametrize("test_path", ["my_path/here"])
def test_add_plugin_path_adds_plugin_path(test_path):
    """Test the plugin add path to be called with the build path."""
    nuke_mock = MagicMock()
    plugin_path = test_path
    os_path_mock = MagicMock()
    os_path_mock.isdir.return_value = True

    with patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock), patch(
        "opendefocus_plugin._plugin_loader._build_plugin_path",
        return_value=plugin_path,
    ), patch(
        "opendefocus_plugin._plugin_loader.os.path.isdir",
        os_path_mock.isdir,
    ):
        add_plugin_path()

    nuke_mock.pluginAppendPath.assert_called_once_with(test_path)
    assert os.getenv("OPENDEFOCUS_LOADED") == "1"


def test_add_plugin_path_with_nonexisting_path():
    """Test the plugin add path to fail if the plugin path is not on disk."""
    nuke_mock = MagicMock()
    nuke_mock.NUKE_VERSION_STRING = "test_version"
    os_path_mock = MagicMock()
    os_path_mock.isdir.return_value = False
    with pytest.raises(
        PluginNotFoundError,
        match="OpenDefocus is installed, "
        "however this version of Nuke: 'test_version' "
        "is not supported in this release. "
        "Please make sure to update to the latest version "
        "of OpenDefocus.",
    ), patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock), patch(
        "opendefocus_plugin._plugin_loader._build_plugin_path",
        return_value="some/path",
    ), patch(
        "opendefocus_plugin._plugin_loader.os.path.isdir",
        os_path_mock.isdir,
    ):
        add_plugin_path()

    nuke_mock.pluginAddPath.assert_not_called()
    assert os.getenv("OPENDEFOCUS_LOADED") == "0"


def test_add_plugin_path_safe():
    """Test the safe adding of plugin path and calling the logger."""
    nuke_mock = MagicMock()
    nuke_mock.NUKE_VERSION_STRING = "test_version"
    os_path_mock = MagicMock()
    os_path_mock.isdir.return_value = False
    logger_mock = MagicMock()

    with patch("opendefocus_plugin._plugin_loader.nuke", nuke_mock), patch(
        "opendefocus_plugin._plugin_loader._build_plugin_path",
        return_value="test/path",
    ), patch("opendefocus_plugin._plugin_loader.logger", logger_mock), patch(
        "opendefocus_plugin._plugin_loader.os.path.isdir",
        os_path_mock.isdir,
    ):
        add_plugin_path_safe()

    logger_mock.exception.assert_called_once_with("Plugin loading failed.")
    assert os.getenv("OPENDEFOCUS_LOADED") == "0"
