import nuke

from opendefocus.datamodel import NODE_CLASS_NAME


def setup_knob_changed():
    # type: () -> None
    """Registration of knob changed callback to Nuke."""
    nuke.addKnobChanged(_knob_changed, nodeClass=NODE_CLASS_NAME)


def _knob_changed():
    # type: () -> None
    """Process incoming knob changed event."""
    knob = nuke.thisKnob()
    if not knob:
        return
    if knob.name() == "inputChange":
        _node_input_change()


def _node_input_change():
    # type: () -> None
    """Set mode and filter type according to input change event."""
    node = nuke.thisNode()
    if not node:
        return
    if node.input(2):
        node["mode"].setValue("camera")
    if node.input(1):
        node["filter_type"].setValue("image")
