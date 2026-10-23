# Headless Natron leak repro for org.opendefocus.OpenDefocus.
# Env: OD_W, OD_H (size), OD_Q (quality), OD_DEPTH (1 = depth connected, 0 = not),
#      OD_MODE (vary = maxSize changes every frame, fixed = unchanged), OD_N (frames), OD_OUT (output dir)
# Run via guard.sh (RSS watchdog). Prints "FRAME i rss_mb=..." after each frame.
import os
import sys

W = int(os.environ.get("OD_W", "1920"))
H = int(os.environ.get("OD_H", "1080"))
Q = int(os.environ.get("OD_Q", "0"))
DEPTH = os.environ.get("OD_DEPTH", "1") == "1"
MODE = os.environ.get("OD_MODE", "vary")
N = int(os.environ.get("OD_N", "30"))
OUT = os.environ.get("OD_OUT", os.path.expanduser("~/od-target/gui/repro/out"))
os.makedirs(OUT, exist_ok=True)


def rss_mb():
    return int(os.popen("ps -o rss= -p %d" % os.getpid()).read().strip() or 0) // 1024


src = app1.createNode("net.sf.openfx.ConstantPlugin")
src.getParam("size").setValue(float(W), 0)
src.getParam("size").setValue(float(H), 1)
src.getParam("color").setValue(0.5, 0)
src.getParam("color").setValue(0.5, 1)
src.getParam("color").setValue(0.5, 2)
src.getParam("color").setValue(1.0, 3)
dep = app1.createNode("net.sf.openfx.CheckerBoardPlugin")
dep.getParam("size").setValue(float(W), 0)
dep.getParam("size").setValue(float(H), 1)
dep.getParam("boxSize").setValue(64.0, 0)
dep.getParam("boxSize").setValue(64.0, 1)


# Reformat (to box W x H) on both inputs so the render size is W x H regardless of project format.
def boxed(n):
    r = app1.createNode("net.sf.openfx.Reformat")
    r.getParam("reformatType").setValue(1)
    r.getParam("boxSize").setValue(float(W), 0)
    r.getParam("boxSize").setValue(float(H), 1)
    r.connectInput(0, n)
    return r


node = app1.createNode("org.opendefocus.OpenDefocus")
node.connectInput(0, boxed(src))
if DEPTH:
    node.connectInput(1, boxed(dep))
for n, v in [
    ("defocusMode", 1),
    ("directCoc", True),
    ("maxSize", 24.0),
    ("cameraMaxSize", 24.0),
    ("quality", Q),
]:
    node.getParam(n).setValue(v)
w = app1.createNode("fr.inria.openfx.WriteOIIO")
w.getParam("filename").setValue(os.path.join(OUT, "f_##.exr"))
w.getParam("bitDepth").setValue(7)
w.connectInput(0, node)
print("CFG", W, H, Q, DEPTH, MODE, N, "rss0_mb=%d" % rss_mb())
sys.stdout.flush()
for i in range(1, N + 1):
    if MODE == "vary":
        node.getParam("maxSize").setValue(20.0 + (i % 5))
    app1.render(w, i, i)
    print("FRAME", i, "rss_mb=%d" % rss_mb())
    sys.stdout.flush()
    for f in (
        [] if os.environ.get("OD_KEEP") == "1" else os.listdir(OUT)
    ):  # keep disk use bounded
        os.remove(os.path.join(OUT, f))
print("DONE rss_mb=%d" % rss_mb())
sys.stdout.flush()
quit()
