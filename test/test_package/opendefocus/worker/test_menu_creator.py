import os
import sys

try:
    from unittest.mock import MagicMock, patch, call
except ImportError:
    from mock import patch, MagicMock, call
import pytest

sys.modules["nuke"] = MagicMock()

from opendefocus.worker.menu_creator import (
    _add_menu_dependancies_to_plugin_path,
    _create_menu,
    add_menu,
)


@pytest.mark.parametrize("plugin_loaded", [(True, False)])
def test_add_menu(plugin_loaded):
    # type: (bool) -> None
    """Test the add menu to be called if the plugin is loaded."""
    os.environ["OPENDEFOCUS_LOADED"] = "1" if plugin_loaded else "0"

    with patch(
        "opendefocus.worker.menu_creator._create_menu"
    ) as create_menu_mock, patch(
        "opendefocus.worker.menu_creator._add_menu_dependancies_to_plugin_path"
    ) as add_resources_mock:
        add_menu()

    if plugin_loaded:
        create_menu_mock.assert_called_once()
        add_resources_mock.assert_called_once()
    else:
        create_menu_mock.assert_not_called()
        add_resources_mock.assert_not_called()


def test_create_menu():
    """Test the menu creation"""
    nuke_mock = MagicMock()

    toolbar_mock = MagicMock()
    menu_mock = MagicMock()
    nuke_mock.menu.return_value = toolbar_mock
    toolbar_mock.addMenu.return_value = menu_mock

    with patch("opendefocus.worker.menu_creator.nuke", nuke_mock):
        _create_menu()

    nuke_mock.menu.assert_called_once_with("Nodes")
    toolbar_mock.addMenu.assert_called_once_with("OpenDefocus", icon="OpenDefocus.png")
    menu_mock.addCommand.assert_called_once_with(
        "OpenDefocus", "nuke.createNode('OpenDefocus')"
    )


def test__add_menu_dependancies_to_plugin_path():
    """Test to add resources during menu loading."""
    nuke_mock = MagicMock()
    installation_path_mock = "test_path"
    with patch("opendefocus.worker.menu_creator.nuke", nuke_mock), patch(
        "opendefocus.worker.menu_creator.INSTALLATION_PATH", installation_path_mock
    ):
        _add_menu_dependancies_to_plugin_path()

    resources_path = os.path.join(installation_path_mock, "resources").replace(
        os.sep, "/"
    )
    python_packages_path = os.path.join(
        installation_path_mock, "python_packages"
    ).replace(os.sep, "/")

    expected_calls = [call(str(resources_path)), call(str(python_packages_path))]
    nuke_mock.pluginAppendPath.assert_has_calls(expected_calls, any_order=False)
