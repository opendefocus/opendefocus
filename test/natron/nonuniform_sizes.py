# Headless Natron matrix for OpenDefocus non-uniform effects at large CoC sizes and amounts.
# One process renders one group (source x frame size x quality) for several CoC values.
# Run via a watchdog wrapper (kill above a memory cap), e.g.:
#   OD_SRC=checker OD_W=1920 OD_H=1080 OD_Q=0 OD_COCS=24,64,128 OD_OUT=~/od-target/nonuni/out/x \
#   OFX_PLUGIN_PATH=~/od-target/ofx NatronRenderer -t nonuniform_sizes.py </dev/null
# Env:
#   OD_SRC   checker | hdr_native | hdr_up   (hdr_up = HDR scene Reformat-boxed to W x H on both inputs)
#   OD_W,OD_H frame size; OD_Q quality index (0 Low, 2 High)
#   OD_COCS  comma list of CoC values; for checker the constant depth = CoC, for HDR maxSize = CoC
#   OD_OUT   output directory (EXR per render, plus RES lines on stdout)
#   OD_ONLY  optional substring filter on config names (e.g. "baseline,all_on")
#   OD_EXPORT_INPUTS=1  write the boxed src/dep EXRs to OD_OUT (for CLI runs on identical inputs) and quit
# Output lines: "RES <tag> <OK|ERROR> <wall_s> <rss_mb> <error>"
import os
import sys
import time

SRC = os.environ.get("OD_SRC", "checker")
W = int(os.environ.get("OD_W", "1920"))
H = int(os.environ.get("OD_H", "1080"))
Q = int(os.environ.get("OD_Q", "0"))
COCS = [float(x) for x in os.environ.get("OD_COCS", "24,64,128").split(",")]
OUT = os.path.expanduser(os.environ.get("OD_OUT", "~/od-target/nonuni/out"))
ONLY = [s for s in os.environ.get("OD_ONLY", "").split(",") if s]
HDR = os.path.expanduser("~/opendefocus/test/images/scenes/hdr_points")
os.makedirs(OUT, exist_ok=True)


def rss_mb():
    return int(os.popen("ps -o rss= -p %d" % os.getpid()).read().strip() or 0) // 1024


def boxed(n):
    # Reformat to a W x H box so inputs match the frame size regardless of project format.
    r = app1.createNode("net.sf.openfx.Reformat")
    r.getParam("reformatType").setValue(1)
    r.getParam("boxSize").setValue(float(W), 0)
    r.getParam("boxSize").setValue(float(H), 1)
    r.getParam("boxFixed").setValue(True)  # exact W x H (no aspect fit)
    r.getParam("resize").setValue(5)  # 5 = distort
    r.connectInput(0, n)
    return r


# ---- graph -------------------------------------------------------------------
if SRC == "checker":
    s = app1.createNode("net.sf.openfx.CheckerBoardPlugin")
    s.getParam("size").setValue(float(W), 0)
    s.getParam("size").setValue(float(H), 1)
    s.getParam("boxSize").setValue(48.0, 0)
    s.getParam("boxSize").setValue(48.0, 1)
    src = boxed(s)
    dsrc = app1.createNode("net.sf.openfx.ConstantPlugin")  # constant direct CoC depth
    dsrc.getParam("size").setValue(float(W), 0)
    dsrc.getParam("size").setValue(float(H), 1)
    dep = boxed(dsrc)
    const = dsrc
else:
    r1 = app1.createNode("fr.inria.openfx.ReadOIIO")
    r1.getParam("filename").setValue(os.path.join(HDR, "color.exr"))
    r2 = app1.createNode("fr.inria.openfx.ReadOIIO")
    r2.getParam("filename").setValue(os.path.join(HDR, "coc.exr"))
    if SRC == "hdr_up":
        src, dep = boxed(r1), boxed(r2)
    else:
        src, dep = r1, r2
    const = None

node = app1.createNode("org.opendefocus.OpenDefocus")
node.connectInput(0, src)
node.connectInput(1, dep)
w = app1.createNode("fr.inria.openfx.WriteOIIO")
w.getParam("bitDepth").setValue(7)
w.connectInput(0, node)
# OD_EXPORT_INPUTS=1: write the (Reformat-boxed) source and depth to OUT and quit, for CLI runs on the same inputs.
if os.environ.get("OD_EXPORT_INPUTS") == "1":
    if const is not None:
        for k in range(3):
            const.getParam("color").setValue(COCS[0], k)
    for nm, n in (("src", src), ("dep", dep)):
        ww = app1.createNode("fr.inria.openfx.WriteOIIO")
        ww.getParam("filename").setValue(
            os.path.join(OUT, "input_%s_%dx%d.exr" % (nm, W, H))
        )
        ww.getParam("bitDepth").setValue(7)
        ww.connectInput(0, n)
        app1.render(ww, 1, 1)
    print("EXPORTED", OUT)
    sys.stdout.flush()
    quit()

# ---- configs: every non-uniform param is set explicitly on each render ----------
BASE = {
    "defocusMode": 1,
    "directCoc": True,
    "quality": Q,
    "catseyeEnable": False,
    "catseyeAmount": 0.5,
    "catseyeGamma": 1.0,
    "catseyeSoftness": 0.2,
    "catseyeInverse": False,
    "axialAberrationEnable": False,
    "axialAberrationAmount": 0.5,
    "axialAberrationColorType": 0,
    "astigmatismEnable": False,
    "astigmatismAmount": 0.5,
    "astigmatismGamma": 1.0,
    "barndoorsEnable": False,
    "barndoorsAmount": 0.5,
    "barndoorsGamma": 1.0,
    "barndoorsInverse": False,
    "barndoorsTop": 100.0,
    "barndoorsBottom": 100.0,
    "barndoorsLeft": 100.0,
    "barndoorsRight": 100.0,
}


def cfg(**kw):
    d = dict(BASE)
    d.update(kw)
    return d


def ca(a, **kw):
    return cfg(catseyeEnable=True, catseyeAmount=a, **kw)


def ast(a, **kw):
    return cfg(astigmatismEnable=True, astigmatismAmount=a, **kw)


def bd(a, side=100.0, **kw):
    return cfg(
        barndoorsEnable=True,
        barndoorsAmount=a,
        barndoorsTop=side,
        barndoorsBottom=side,
        barndoorsLeft=side,
        barndoorsRight=side,
        **kw,
    )


def ax(t, a):
    return cfg(
        axialAberrationEnable=True, axialAberrationAmount=a, axialAberrationColorType=t
    )


AMOUNTS = [0.0, 0.5, 2.0, 5.0, 10.0]
CONFIGS = [("baseline", cfg())]
# catseye plugin hard max is 2.0 (Natron clamps above); CLI covers 20/50/100 separately
CA_AMOUNTS = [0.0, 0.5, 2.0]
CONFIGS += [(f"catseye_a{a:g}", ca(a)) for a in CA_AMOUNTS]
CONFIGS += [
    ("catseye_inv_a0.5", ca(0.5, catseyeInverse=True)),
    ("catseye_inv_a2", ca(2.0, catseyeInverse=True)),
]
CONFIGS += [(f"astig_g1_a{a:g}", ast(a)) for a in AMOUNTS]
CONFIGS += [
    ("astig_g3_a0.5", ast(0.5, astigmatismGamma=3.0)),
    ("astig_g3_a5", ast(5.0, astigmatismGamma=3.0)),
]
CONFIGS += [(f"barn_s100_a{a:g}", bd(a)) for a in AMOUNTS]
CONFIGS += [
    ("barn_s1000_a0.5", bd(0.5, 1000.0)),
    ("barn_s1000_a5", bd(5.0, 1000.0)),
    ("barn_inv_a5", bd(5.0, 100.0, barndoorsInverse=True)),
]
CONFIGS += [("axial_t%d_a%g" % (t, a), ax(t, a)) for t in (0, 1, 2) for a in (0.5, 1.0)]
CONFIGS += [
    (
        "all_on",
        cfg(
            catseyeEnable=True,
            catseyeAmount=2.0,
            astigmatismEnable=True,
            astigmatismAmount=5.0,
            astigmatismGamma=3.0,
            barndoorsEnable=True,
            barndoorsAmount=5.0,
            axialAberrationEnable=True,
            axialAberrationAmount=1.0,
            axialAberrationColorType=0,
        ),
    )
]
if ONLY:
    CONFIGS = [c for c in CONFIGS if any(o in c[0] for o in ONLY) or c[0] == "baseline"]

# ---- render loop -----------------------------------------------------------------
gtag = "%s_%dx%d_q%d" % (SRC, W, H, Q)
for coc in COCS:
    if const is not None:
        for k in range(3):
            const.getParam("color").setValue(coc, k)
        const.getParam("color").setValue(1.0, 3)
    # HDR depth is coc.exr; there the CoC value is applied through maxSize only
    for name, params in CONFIGS:
        tag = "%s_c%d_%s" % (gtag, int(coc), name)
        p = dict(params)
        p["maxSize"] = coc
        p["cameraMaxSize"] = coc
        for k, v in p.items():
            node.getParam(k).setValue(v)
        w.getParam("filename").setValue(os.path.join(OUT, tag + ".exr"))
        t0 = time.time()
        status = "OK"
        err = ""
        try:
            app1.render(w, 1, 1)
        except Exception as e:
            status = "ERROR"
            err = str(e)[:200].replace("\n", " ")
        wall = time.time() - t0
        print("RES %s %s %.2f %d %s" % (tag, status, wall, rss_mb(), err))
        sys.stdout.flush()

print("DONE %s rss_mb=%d" % (gtag, rss_mb()))
sys.stdout.flush()
quit()
