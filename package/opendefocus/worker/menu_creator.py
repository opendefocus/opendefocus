"""Copyright (c) 2025 OpenDefocus.  All Rights Reserved.

Functions that handles the creation of the Nuke menu.
"""

import os

import nuke

from opendefocus.datamodel import INSTALLATION_PATH


def _create_menu():
    # type: () -> None
    """Create the Nuke menu and add the command"""
    toolbar = nuke.menu("Nodes")
    menu = toolbar.addMenu("OpenDefocus", icon="OpenDefocus.png")
    menu.addCommand("OpenDefocus", "nuke.createNode('OpenDefocus')")


def add_menu():
    # type: () -> None
    """Add the Nuke menu if OpenDefocus is found."""
    if os.getenv("OPENDEFOCUS_LOADED") != "1":
        return

    _add_menu_dependancies_to_plugin_path()
    _set_installation_directory()
    _create_menu()


def _add_menu_dependancies_to_plugin_path():
    # type: () -> None
    nuke.pluginAppendPath(os.path.join(INSTALLATION_PATH, "resources").replace(os.sep, "/"))
    nuke.pluginAppendPath(os.path.join(INSTALLATION_PATH, "python_packages").replace(os.sep, "/"))


def _set_installation_directory():
    # type: () -> None
    os.environ["OPENDEFOCUS_INSTALLATION"] = str(INSTALLATION_PATH)
