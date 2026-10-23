# Run by `cargo xtask --test-natron`, which sets OPENDEFOCUS_REPO and OPENDEFOCUS_NATRON_OUT.
import os
import sys

repo = os.environ["OPENDEFOCUS_REPO"]
sc = os.path.join(repo, "test", "images", "scenes", "occlusion") + "/"
out = os.environ["OPENDEFOCUS_NATRON_OUT"]
read = app1.createNode("fr.inria.openfx.ReadOIIO")
read.getParam("filename").setValue(sc + "color.exr")
depth = app1.createNode("fr.inria.openfx.ReadOIIO")
depth.getParam("filename").setValue(sc + "coc.exr")
node = app1.createNode("org.opendefocus.OpenDefocus")
node.connectInput(0, read)
node.connectInput(1, depth)
for n, v in [
    ("defocusMode", 1),
    ("directCoc", True),
    ("maxSize", 24.0),
    ("cameraMaxSize", 24.0),
    ("quality", 0),
]:
    p = node.getParam(n)
    print("SET", n, v, p is not None)
    p.setValue(v)
w = app1.createNode("fr.inria.openfx.WriteOIIO")
w.getParam("filename").setValue(out)
bd = w.getParam("bitDepth")
bd.setValue(7)
print("BITDEPTH", bd.getOptions() if bd else None)
w.connectInput(0, node)
app1.render(w, 1, 1)
print("DONE")
sys.stdout.flush()
quit()
