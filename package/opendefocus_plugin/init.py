"""Main entry point for OpenDefocus Nuke plugin."""

import logging

from opendefocus_plugin._node_setup import setup_knob_changed
from opendefocus_plugin._plugin_loader import (
    add_plugin_path_safe,
)

FORMAT = "[%(asctime)s] %(message)s"
logging.basicConfig(level=logging.INFO, format=FORMAT)


add_plugin_path_safe()
setup_knob_changed()
