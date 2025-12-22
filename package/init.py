"""OpenDefocus init.py to load plugin in Nuke."""

import nuke  # ty:ignore[unresolved-import]

# Either add this line to your init.py or use this entire file.
# OpenDefocus will handle everything else automatically.
nuke.pluginAddPath("./opendefocus_plugin")  # ty:ignore[unresolved-attribute]
