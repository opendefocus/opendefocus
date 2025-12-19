import logging

from opendefocus.worker.node_setup import setup_knob_changed
from opendefocus.worker.plugin_loader import (
    add_plugin_path_safe,
)

FORMAT = "[%(asctime)s] %(message)s"
logging.basicConfig(level=logging.INFO, format=FORMAT)


add_plugin_path_safe()
setup_knob_changed()
