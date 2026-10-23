# Headless parameter sweep for org.opendefocus.OpenDefocus (NatronRenderer -t).
#
# Run:
#   OFX_PLUGIN_PATH=~/od-target/ofx ~/Applications/Natron.app/Contents/MacOS/NatronRenderer \
#       -t ~/opendefocus/test/natron/param_sweep.py </dev/null > LOG 2>&1
#
# Env:
#   SWEEP_OUT=DIR          output directory (default ~/od-target/sweep)
#   SWEEP_ONLY=id,id       render only these case ids (repro of a single case)
#   SWEEP_RESUME=1         skip case ids already in DIR/manifest.jsonl (after a crash)
#   SWEEP_ANALYZE_ONLY=1   no rendering; rebuild the summary from DIR/manifest.jsonl
#   SWEEP_LOG=PATH         the NatronRenderer log; its lines are attributed to cases
#   SWEEP_SUBSET=p1,p2     replace the all_max case with B*:subset:p1+p2 (those params at max)
#   SWEEP_SET=p=v,p=v      add case B3:set:p-v+p-v with explicit values (others at baseline)
#
# Each case resets every plugin parameter to its baseline value, applies the case overrides,
# and renders with fr.inria.openfx.WriteOIIO (bitDepth 32f). Analysis uses iinfo --stats and
# idiff from the Natron bundle. Outputs: DIR/img/*.exr, DIR/manifest.jsonl, DIR/summary.txt,
# DIR/results.json, DIR/inventory.tsv.

import hashlib
import json
import os
import re
import subprocess
import time
import traceback

HOME = os.path.expanduser("~")
REPO = os.path.join(HOME, "opendefocus")
SCENES = os.path.join(REPO, "test", "images", "scenes")
TOOLS = os.path.join(HOME, "Applications", "Natron.app", "Contents", "MacOS")
OUT = os.path.expanduser(os.environ.get("SWEEP_OUT", "~/od-target/sweep"))
IMG = os.path.join(OUT, "img")
MANIFEST = os.path.join(OUT, "manifest.jsonl")
PLUGIN = "org.opendefocus.OpenDefocus"
DEFAULT = "default"  # sentinel: use the baseline's depth connection

# Plugin rows from params.rs (pages and groups excluded). Checked against node.getParams().
PLUGIN_PARAMS = [
    "defocusMode",
    "size",
    "maxSize",
    "focalPlane",
    "quality",
    "math",
    "directCoc",
    "protect",
    "sizeMultiplier",
    "gamma",
    "cameraEnabled",
    "focalLength",
    "cameraMaxSize",
    "fStop",
    "filmbackWidth",
    "filmbackHeight",
    "worldUnit",
    "inverseForeground",
    "bokehFilterType",
    "bokehRadius",
    "bokehRingColor",
    "bokehInnerColor",
    "bokehRingSize",
    "bokehOuterBlur",
    "bokehInnerBlur",
    "bokehBlades",
    "bokehAngle",
    "bokehCurvature",
    "bokehAspectRatio",
    "bokehNoiseSize",
    "bokehNoiseIntensity",
    "bokehNoiseOctaves",
    "bokehNoiseSeed",
    "catseyeEnable",
    "catseyeAmount",
    "catseyeGamma",
    "catseyeSoftness",
    "catseyeInverse",
    "axialAberrationEnable",
    "axialAberrationAmount",
    "axialAberrationColorType",
    "astigmatismEnable",
    "astigmatismAmount",
    "astigmatismGamma",
    "barndoorsEnable",
    "barndoorsAmount",
    "barndoorsGamma",
    "barndoorsInverse",
    "barndoorsTop",
    "barndoorsBottom",
    "barndoorsLeft",
    "barndoorsRight",
]

READ_FILES = {
    "color": os.path.join(SCENES, "occlusion", "color.exr"),
    "coc": os.path.join(SCENES, "occlusion", "coc.exr"),
    "depth": os.path.join(SCENES, "occlusion", "depth.exr"),
    "alpha": os.path.join(SCENES, "alpha_edges", "color.exr"),
    "alpha_coc": os.path.join(SCENES, "alpha_edges", "coc.exr"),
    "toad": os.path.join(REPO, "test", "images", "any", "toad.png"),
}

_DIRECT = {
    "directCoc": True,
    "defocusMode": 1,
    "maxSize": 24.0,
    "cameraMaxSize": 24.0,
    "quality": 0,
}
BASES = {
    # Requested baseline: direct CoC from coc.exr in Depth mode.
    "B1": {"src": "color", "depth": "coc", "over": dict(_DIRECT)},
    # Metric depth (depth.exr, metres), CoC computed from focal plane 4 m (not direct).
    "B2": {
        "src": "color",
        "depth": "depth",
        "over": {
            "directCoc": False,
            "defocusMode": 1,
            "focalPlane": 4.0,
            "maxSize": 24.0,
            "cameraMaxSize": 24.0,
            "quality": 0,
        },
    },
    # alpha_edges scene with the B1 settings.
    "B3": {"src": "alpha", "depth": "alpha_coc", "over": dict(_DIRECT)},
    # toad.png in 2D mode, Depth input disconnected.
    "B4": {
        "src": "toad",
        "depth": None,
        "over": {
            "defocusMode": 0,
            "maxSize": 24.0,
            "cameraMaxSize": 24.0,
            "quality": 0,
        },
    },
}

INFO = {}  # name -> {type, default, min, max, options, canAnimate, dispmin, dispmax}


def safe(text):
    return re.sub(r"[^A-Za-z0-9_.-]+", "_", text)


def fn(f):
    try:
        return f()
    except Exception as e:  # reported as a string, never fatal
        return f"ERR {e}"


def read_param_info(node):
    info = {}
    for p in node.getParams():
        name = p.getScriptName()
        if name not in PLUGIN_PARAMS:
            continue
        t = p.getTypeName()
        d = {
            "type": t,
            "default": fn(p.getDefaultValue),
            "canAnimate": fn(p.getCanAnimate),
        }
        if t in ("Double", "Int"):
            d.update(
                min=p.getMinimum(),
                max=p.getMaximum(),
                dispmin=fn(lambda p=p: p.getDisplayMinimum(0)),
                dispmax=fn(lambda p=p: p.getDisplayMaximum(0)),
            )
        if t == "Choice":
            d["options"] = list(p.getOptions())
        info[name] = d
    missing = [n for n in PLUGIN_PARAMS if n not in info]
    sorted(n for n in info if n not in PLUGIN_PARAMS)
    print("INVENTORY params=%d missing=%s" % (len(info), missing), flush=True)
    with open(os.path.join(OUT, "inventory.tsv"), "w") as f:
        f.write(
            "name\ttype\tmin\tmax\tdefault\tdisp_min\tdisp_max\tcanAnimate\toptions\n"
        )
        for n in PLUGIN_PARAMS:
            d = info.get(n)
            if d is None:
                continue
            f.write(
                "\t".join(
                    str(x)
                    for x in [
                        n,
                        d["type"],
                        d.get("min", ""),
                        d.get("max", ""),
                        d["default"],
                        d.get("dispmin", ""),
                        d.get("dispmax", ""),
                        d["canAnimate"],
                        "|".join(d.get("options", [])),
                    ]
                )
                + "\n"
            )
    return info


def variants(d):
    t = d["type"]
    if t in ("Double", "Int"):
        cast = float if t == "Double" else int
        lo, hi = d["min"], d["max"]
        return [
            ("min", cast(lo)),
            ("max", cast(hi)),
            ("oob_lo", cast(lo - 1)),
            ("oob_hi", cast(hi + 1)),
        ]
    if t == "Bool":
        return [("false", False), ("true", True)]
    opts = d["options"]
    return [("%d:%s" % (i, o), i) for i, o in enumerate(opts)] + [
        ("oob_hi", len(opts)),
        ("oob_lo", -1),
    ]


def coerce(d, v):
    t = d["type"]
    if t == "Double":
        return float(v)
    if t == "Bool":
        return bool(v)
    return int(v)


def base_values(base):
    vals = {n: d["default"] for n, d in INFO.items()}
    vals.update(BASES[base]["over"])
    return vals


def extreme(which):
    out = {}
    for n, d in INFO.items():
        t = d["type"]
        if t in ("Double", "Int"):
            out[n] = d["min"] if which == "min" else d["max"]
        elif t == "Bool":
            out[n] = which == "max"
        else:
            out[n] = 0 if which == "min" else len(d["options"]) - 1
    return out


def readback(p):
    for m in ("getValue", "get"):
        f = getattr(p, m, None)
        if f is not None:
            v = fn(f)
            return v if isinstance(v, (bool, int, float, str)) else str(v)
    return "ERR no getter"


def make_reads():
    reads = {}
    for key, path in READ_FILES.items():
        r = app1.createNode("fr.inria.openfx.ReadOIIO")
        r.getParam("filename").setValue(path)
        reads[key] = r
    return reads


def make_graph():
    node = app1.createNode(PLUGIN)
    w = app1.createNode("fr.inria.openfx.WriteOIIO")
    w.getParam("bitDepth").setValue(7)
    w.connectInput(0, node)
    return node, w


def build_cases():
    cases = []
    for base in BASES:
        cases.append({"id": base + ":baseline", "base": base})
    for base in ("B1", "B2"):
        for n in PLUGIN_PARAMS:
            for label, v in variants(INFO[n]):
                cases.append(
                    {"id": f"{base}:{n}={label}", "base": base, "over": {n: v}}
                )

    enables = [
        "catseyeEnable",
        "axialAberrationEnable",
        "astigmatismEnable",
        "barndoorsEnable",
    ]
    inverses = ["catseyeInverse", "barndoorsInverse"]
    amounts = [
        "catseyeAmount",
        "axialAberrationAmount",
        "astigmatismAmount",
        "barndoorsAmount",
    ]
    gammas = ["catseyeGamma", "astigmatismGamma", "barndoorsGamma"]
    en_on = {n: True for n in enables + inverses}
    en_max = dict(en_on)
    en_max.update({n: INFO[n]["max"] for n in amounts + gammas})
    subset = [x for x in os.environ.get("SWEEP_SUBSET", "").split(",") if x]
    for n in subset:
        if n not in PLUGIN_PARAMS:
            raise SystemExit(f"SWEEP_SUBSET: unknown param {n}")
    setspec = [
        kv.split("=", 1) for kv in os.environ.get("SWEEP_SET", "").split(",") if kv
    ]
    for (
        n,
        _,
    ) in (
        setspec
    ):  # scaling runs: explicit values, e.g. SWEEP_SET="maxSize=300,sizeMultiplier=100"
        if n not in PLUGIN_PARAMS:
            raise SystemExit(f"SWEEP_SET: unknown param {n}")
    if setspec:
        cases.append(
            {
                "id": "B3:set:"
                + "+".join("{}-{}".format(*tuple(kv)) for kv in setspec),
                "base": "B3",
                "over": {n: float(v) for n, v in setspec},
            }
        )
    for base in BASES:
        cases.append({"id": base + ":all_enables_on", "base": base, "over": en_on})
        cases.append({"id": base + ":all_enables_max", "base": base, "over": en_max})
        cases.append({"id": base + ":all_min", "base": base, "over": extreme("min")})
        if subset:  # bisection: only these params at max, the rest at baseline
            mx = extreme("max")
            cases.append(
                {
                    "id": base + ":subset:" + "+".join(subset),
                    "base": base,
                    "over": {n: mx[n] for n in subset},
                }
            )
        else:
            cases.append(
                {"id": base + ":all_max", "base": base, "over": extreme("max")}
            )
    for base in ("B1", "B2"):
        cases.append(
            {"id": base + ":quality_accurate", "base": base, "over": {"quality": 1}}
        )

    # Camera: focal distance 0 and positive, camera on and off.
    cases += [
        {
            "id": "B2:cam_on_focalPlane0",
            "base": "B2",
            "over": {"cameraEnabled": True, "focalPlane": 0.0},
        },
        {
            "id": "B2:cam_on_focalLength0",
            "base": "B2",
            "over": {"cameraEnabled": True, "focalLength": 0.0},
        },
        {
            "id": "B2:cam_on_focal50_f1.4",
            "base": "B2",
            "over": {"cameraEnabled": True, "focalLength": 50.0, "fStop": 1.4},
        },
        {
            "id": "B2:cam_on_focal300_cmax100",
            "base": "B2",
            "over": {
                "cameraEnabled": True,
                "focalLength": 300.0,
                "fStop": 2.8,
                "cameraMaxSize": 100.0,
            },
        },
        {
            "id": "B2:cam_on_worldUnit_ft",
            "base": "B2",
            "over": {"cameraEnabled": True, "worldUnit": 5},
        },
        {
            "id": "B2:cam_off_focalLength0",
            "base": "B2",
            "over": {"cameraEnabled": False, "focalLength": 0.0},
        },
        {
            "id": "B1:cam_on_focalPlane0",
            "base": "B1",
            "over": {"cameraEnabled": True, "focalPlane": 0.0},
        },
        {
            "id": "B1:cam_on_focal50",
            "base": "B1",
            "over": {"cameraEnabled": True, "focalLength": 50.0},
        },
    ]

    # Bokeh shape and noise.
    noise_max = {
        "bokehFilterType": 1,
        "bokehNoiseSize": INFO["bokehNoiseSize"]["max"],
        "bokehNoiseIntensity": INFO["bokehNoiseIntensity"]["max"],
        "bokehNoiseOctaves": INFO["bokehNoiseOctaves"]["max"],
        "bokehNoiseSeed": INFO["bokehNoiseSeed"]["max"],
    }
    cases += [
        {
            "id": "B2:blade3",
            "base": "B2",
            "over": {"bokehFilterType": 1, "bokehBlades": 3},
        },
        {
            "id": "B2:blade16",
            "base": "B2",
            "over": {"bokehFilterType": 1, "bokehBlades": 16},
        },
        {
            "id": "B2:blade64",
            "base": "B2",
            "over": {"bokehFilterType": 1, "bokehBlades": 64},
        },
        {
            "id": "B2:disc_blades64",
            "base": "B2",
            "over": {"bokehFilterType": 0, "bokehBlades": 64},
        },
        {
            "id": "B2:blade_aspect0",
            "base": "B2",
            "over": {"bokehFilterType": 1, "bokehAspectRatio": 0.0},
        },
        {
            "id": "B1:blade16",
            "base": "B1",
            "over": {"bokehFilterType": 1, "bokehBlades": 16},
        },
        {"id": "B2:noise_max", "base": "B2", "over": dict(noise_max)},
        {"id": "B1:noise_max", "base": "B1", "over": dict(noise_max)},
    ]

    # Astigmatism on at maximum strength; the full RoD of each source is rendered.
    astig = {
        "astigmatismEnable": True,
        "astigmatismAmount": INFO["astigmatismAmount"]["max"],
        "astigmatismGamma": INFO["astigmatismGamma"]["max"],
    }
    for base in BASES:
        cases.append({"id": base + ":astig_max", "base": base, "over": dict(astig)})

    # Barndoors at the corners of the position range, normal and inverse.
    lo, hi = INFO["barndoorsTop"]["min"], INFO["barndoorsTop"]["max"]
    corners = [
        ("all_lo", (lo, lo, lo, lo)),
        ("all_hi", (hi, hi, hi, hi)),
        ("tb_lo_lr_hi", (lo, lo, hi, hi)),
        ("tb_hi_lr_lo", (hi, hi, lo, lo)),
    ]
    for inv in (False, True):
        for cname, (t, b, left, r) in corners:
            cases.append(
                {
                    "id": "B2:barndoors_%s_inv%d" % (cname, int(inv)),
                    "base": "B2",
                    "over": {
                        "barndoorsEnable": True,
                        "barndoorsInverse": inv,
                        "barndoorsTop": t,
                        "barndoorsBottom": b,
                        "barndoorsLeft": left,
                        "barndoorsRight": r,
                    },
                }
            )
    for cname, (t, b, left, r) in corners[:2]:
        cases.append(
            {
                "id": f"B1:barndoors_{cname}",
                "base": "B1",
                "over": {
                    "barndoorsEnable": True,
                    "barndoorsTop": t,
                    "barndoorsBottom": b,
                    "barndoorsLeft": left,
                    "barndoorsRight": r,
                },
            }
        )

    # Depth input disconnected.
    cases += [
        {"id": "B1:depth_disconnected", "base": "B1", "depth": None, "over": {}},
        {
            "id": "B1:depth_disconnected_mode0",
            "base": "B1",
            "depth": None,
            "over": {"defocusMode": 0},
        },
        {
            "id": "B2:depth_disconnected_cam",
            "base": "B2",
            "depth": None,
            "over": {"cameraEnabled": True},
        },
        {
            "id": "B4:depth_disconnected_mode1",
            "base": "B4",
            "depth": None,
            "over": {"defocusMode": 1},
        },
    ]

    # Animation: keyframes on size and focalPlane over frames 1..5, rendered as a sequence.
    size_keys = [(1, 0.0), (2, 2.0), (3, 5.0), (4, 10.0), (5, 20.0)]
    focal_keys = [(1, 0.0), (2, 2.0), (3, 4.0), (4, 6.0), (5, 8.0)]
    cases += [
        {
            "id": "B2:anim_size",
            "base": "B2",
            "fresh": True,
            "frames": (1, 5),
            "pattern": "anim_b2size_####.exr",
            "keys": {"size": size_keys},
        },
        {
            "id": "B2:anim_focalPlane",
            "base": "B2",
            "fresh": True,
            "frames": (1, 5),
            "pattern": "anim_b2focal_####.exr",
            "keys": {"focalPlane": focal_keys},
        },
        {
            "id": "B1:anim_size",
            "base": "B1",
            "fresh": True,
            "frames": (1, 5),
            "pattern": "anim_b1size_####.exr",
            "keys": {"size": size_keys},
        },
    ]
    return cases


def valat(p, t):
    f = getattr(p, "getValueAtTime", None)
    return fn(lambda: f(t)) if f else "ERR no getValueAtTime"


def run_case(node, w, reads, case, mf):
    cid = case["id"]
    base = case["base"]
    over = dict(case.get("over", {}))
    keys = case.get("keys", {})
    frames = case.get("frames", (1, 1))
    pattern = case.get("pattern")
    depth_key = BASES[base]["depth"] if "depth" not in case else case["depth"]
    bvals = base_values(base)
    if pattern:
        paths = [
            os.path.join(IMG, pattern.replace("####", "%04d" % f))
            for f in range(frames[0], frames[1] + 1)
        ]
        write_path = os.path.join(IMG, pattern)
    else:
        name = safe(cid)
        if len(name) > 120:  # long subset ids exceed the 255-byte filename limit
            name = name[:80] + "_" + hashlib.md5(cid.encode()).hexdigest()[:12]
        write_path = os.path.join(IMG, name + ".exr")
        paths = [write_path]
    for p in paths:
        if os.path.exists(p):
            os.remove(p)

    rec = {
        "id": cid,
        "base": base,
        "over": over,
        "keys": {k: [list(x) for x in v] for k, v in keys.items()},
        "frames": list(frames),
        "depth": depth_key,
        "types": {n: INFO[n]["type"] for n in list(over) + list(keys)},
        "base_value": {n: bvals[n] for n in list(over) + list(keys)},
        "errors": [],
        "status": None,
        "readback": {},
        "keyed": {},
    }
    print("CASE_BEGIN", cid, flush=True)
    t0 = time.time()
    try:
        if depth_key is None:
            rec["disconnect_depth_returned"] = fn(lambda: node.disconnectInput(1))
        else:
            node.connectInput(1, reads[depth_key])
        node.connectInput(0, reads[BASES[base]["src"]])
        vals = dict(bvals)
        vals.update(over)
        for n in PLUGIN_PARAMS:
            node.getParam(n).setValue(coerce(INFO[n], vals[n]))
        for n in over:
            rec["readback"][n] = readback(node.getParam(n))
        for n, lst in keys.items():
            p = node.getParam(n)
            try:
                for t, v in lst:
                    p.setValueAtTime(float(v), t)
            except (
                Exception
            ):  # keep going: the render shows what the host did with the keys
                rec["errors"].append(
                    f"setValueAtTime({n}): {traceback.format_exc(limit=1)[-300:]}"
                )
            rec["keyed"][n] = {
                "isAnimated": fn(p.getIsAnimated),
                "valueAtTime": [valat(p, t) for t, _ in lst],
            }
        w.getParam("filename").setValue(write_path)
        app1.render(w, frames[0], frames[1])
        rec["status"] = "ok"
    except Exception:
        rec["status"] = "exception"
        rec["errors"].append(traceback.format_exc(limit=4)[-700:])
    rec["secs"] = round(time.time() - t0, 2)
    rec["files"] = [p for p in paths if os.path.exists(p) and os.path.getsize(p) > 0]
    if rec["status"] == "ok" and len(rec["files"]) != len(paths):
        rec["status"] = "missing_output"
    print("CASE_END", cid, rec["status"], "files=%d" % len(rec["files"]), flush=True)
    mf.write(json.dumps(rec) + "\n")
    mf.flush()


# ---------------------------------------------------------------- analysis


def iinfo_stats(path):
    r = subprocess.run(
        [os.path.join(TOOLS, "iinfo"), "--stats", path], capture_output=True, text=True
    )
    txt = (r.stdout or "") + (r.stderr or "")
    out = {
        "rc": r.returncode,
        "nan": None,
        "inf": None,
        "max": None,
        "min": None,
        "avg": None,
        "size": None,
    }
    m = re.search(r"(\d+)\s*x\s*(\d+)", txt)
    if m:
        out["size"] = [int(m.group(1)), int(m.group(2))]
    for key, name in (("nan", "NanCount"), ("inf", "InfCount")):
        m = re.search(name + r":\s*([\d ]+)", txt)
        if m:
            out[key] = sum(int(x) for x in m.group(1).split())
    for key, name in (("max", "Max"), ("min", "Min"), ("avg", "Avg")):
        m = re.search(r"Stats " + name + r":\s*([^(\n]*)\(", txt)
        if m:
            vals = [float(x) for x in m.group(1).split()]
            out[key] = (
                max(vals)
                if key == "max"
                else min(vals)
                if key == "min"
                else sum(vals) / len(vals)
            )
    return out


def idiff(a, b):
    r = subprocess.run(
        [os.path.join(TOOLS, "idiff"), a, b], capture_output=True, text=True
    )
    txt = (r.stdout or "") + (r.stderr or "")
    m = re.search(r"Max error\s*=\s*(\S+)", txt)
    if m:
        mean = re.search(r"Mean error\s*=\s*(\S+)", txt)
        return {
            "max": float(m.group(1)),
            "mean": float(mean.group(1)) if mean else None,
        }
    if r.returncode == 0 and "Comparing" in txt:
        return {"max": 0.0, "mean": 0.0}
    return {"error": txt.strip()[-200:]}


def same_value(t, req, got):
    if isinstance(got, str):
        return False
    if t == "Double":
        return abs(float(req) - float(got)) <= 1e-9 * max(1.0, abs(float(req)))
    if t == "Bool":
        return bool(req) == bool(got)
    return int(req) == int(got)


def log_errors():
    res = {}
    path = os.environ.get("SWEEP_LOG", "")
    if not path or not os.path.exists(path):
        return res
    pat = re.compile(
        r"(?i)(error|exception|fail|invalid|assert|abort|panic|traceback|warn|"
        r"opendefocus)"
    )
    cur = None
    with open(path, errors="replace") as f:
        for line in f:
            s = line.rstrip("\n")
            if s.startswith("CASE_BEGIN "):
                cur = s[len("CASE_BEGIN ") :]
                continue
            if s.startswith("CASE_END "):
                cur = None
                continue
            if cur and pat.search(s):
                lst = res.setdefault(cur, [])
                if len(lst) < 5 and s[:300] not in lst:
                    lst.append(s[:300])
    return res


def cell(r):
    if r.get("md") is None:
        s = "-"
    elif "max" in r["md"]:
        s = "md={:.4g}".format(r["md"]["max"])
    else:
        s = "idiff_err"
    for f in r["flags"]:
        if (
            f.startswith("CLAMP")
            or f in ("NO_EFFECT", "FAIL", "NONFINITE")
            or f.startswith("RENDER")
        ):
            s += " " + f
    return s


def report(records):
    by_id = {}
    for r in records:
        by_id[r["id"]] = r  # last record for an id wins
    recs = list(by_id.values())
    logerr = log_errors()
    for r in recs:
        r["flags"] = []
        r["stats"] = [iinfo_stats(f) for f in r.get("files", [])]
        nonfinite = sum((s["nan"] or 0) + (s["inf"] or 0) for s in r["stats"])
        if r["status"] != "ok":
            r["flags"].append("RENDER_" + str(r["status"]).upper())
        if r["files"] and nonfinite:
            r["flags"].append("NONFINITE")
        for n in r["over"]:
            got = r["readback"].get(n)
            if isinstance(got, str) and got.startswith("ERR"):
                r["flags"].append("READBACK_ERR")
            elif not same_value(r["types"][n], r["over"][n], got):
                r["flags"].append("CLAMP({}->{})".format(r["over"][n], got))
        # effective value after readback (the requested value if readback failed)
        eff = {
            n: (
                r["readback"].get(n)
                if not isinstance(r["readback"].get(n), str)
                else r["over"][n]
            )
            for n in r["over"]
        }
        r["md"] = None
        bf = os.path.join(IMG, safe(r["base"] + ":baseline") + ".exr")
        if r["id"].endswith(":baseline") or not r["files"] or not os.path.exists(bf):
            pass
        elif len(r["files"]) == 1:
            r["md"] = idiff(bf, r["files"][0])
        else:
            r["md"] = {
                "max": max(idiff(bf, f).get("max", float("nan")) for f in r["files"])
            }
            r["consecutive"] = [
                idiff(r["files"][i - 1], r["files"][i]).get("max")
                for i in range(1, len(r["files"]))
            ]
        if r["md"] and "max" in r["md"] and r["md"]["max"] != r["md"]["max"]:
            r["flags"].append("NONFINITE_DIFF")
        if r["md"] and "error" in r["md"]:
            r["flags"].append("IDIFF_ERR")
        if r["md"] and r["md"].get("max") == 0 and r["over"]:
            changed = [
                n
                for n in r["over"]
                if not same_value(r["types"][n], eff[n], r["base_value"][n])
            ]
            if changed:
                r["flags"].append("NO_EFFECT")
            else:
                r["flags"].append("SAME_AS_BASE")
        r["log_errors"] = logerr.get(r["id"], [])

    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "results.json"), "w", encoding="utf-8") as f:
        json.dump(recs, f, indent=1, default=str)
    L = []
    counts = {"cases": len(recs)}
    for r in recs:
        for f in r["flags"]:
            key = f.split("(")[0]
            counts[key] = counts.get(key, 0) + 1
    L.append(
        "cases=%d  flags: %s"
        % (
            len(recs),
            ", ".join(
                "%s=%d" % kv for kv in sorted(counts.items()) if kv[0] != "cases"
            ),
        )
    )
    L.append("")
    L.append("== per-parameter results (md = max abs diff vs same-base baseline) ==")
    for n in PLUGIN_PARAMS:
        for base in ("B1", "B2"):
            cells = [
                "{}[{}]".format(r["id"].split("=", 1)[1], cell(r))
                for r in recs
                if r["id"].startswith(f"{base}:{n}=")
            ]
            if cells:
                L.append("%-22s %s  %s" % (n, base, "  ".join(cells)))
    L.append("")
    L.append("== combos, depth-disconnected, animation ==")
    for r in recs:
        if "=" in r["id"] or r["id"].endswith(":baseline"):
            continue  # per-parameter cases are listed above
        extra = ""
        if r.get("consecutive") is not None:
            extra = " frame_diffs=%s" % [
                f"{x:.3g}" if isinstance(x, float) else x for x in r["consecutive"]
            ]
        L.append("%-34s %s %s%s" % (r["id"], r["status"], cell(r), extra))
    L.append("")
    L.append("== failing / suspicious (with first error text and log lines) ==")
    for r in recs:
        bad = [
            f
            for f in r["flags"]
            if f.startswith(("RENDER", "NONFINITE", "READBACK_ERR", "IDIFF_ERR"))
        ]
        if bad or r["log_errors"]:
            L.append("{}  {}".format(r["id"], " ".join(bad)))
            for e in r["errors"][:1]:
                L.append("    py: " + e.strip().splitlines()[-1][:300])
            for e in r["log_errors"][:2]:
                L.append("    log: " + e)
            if bad:
                L.append(
                    "    repro: SWEEP_ONLY='{}' SWEEP_OUT={} OFX_PLUGIN_PATH=~/od-target/ofx "
                    "~/Applications/Natron.app/Contents/MacOS/NatronRenderer -t "
                    "~/opendefocus/test/natron/param_sweep.py </dev/null".format(
                        r["id"], os.path.join(OUT, "repro")
                    )
                )
    text = "\n".join(L) + "\n"
    with open(os.path.join(OUT, "summary.txt"), "w", encoding="utf-8") as f:
        f.write(text)
    # Natron's embedded stdout may be ASCII-only; the file above keeps the full text.
    print(text.encode("ascii", "replace").decode("ascii"), flush=True)


def main():
    os.makedirs(IMG, exist_ok=True)
    if os.environ.get("SWEEP_ANALYZE_ONLY") == "1":
        with open(MANIFEST) as f:
            records = [json.loads(line) for line in f if line.strip()]
        report(records)
        return
    only = set(x for x in os.environ.get("SWEEP_ONLY", "").split(",") if x)
    done = set()
    if os.environ.get("SWEEP_RESUME") == "1" and os.path.exists(MANIFEST):
        with open(MANIFEST) as f:
            done = set(json.loads(line)["id"] for line in f if line.strip())
    reads = make_reads()
    node, w = make_graph()
    INFO.update(read_param_info(node))
    cases = build_cases()
    print("CASES total=%d" % len(cases), flush=True)
    with open(MANIFEST, "a") as mf:
        for case in cases:
            if only and case["id"] not in only:
                continue
            if case["id"] in done:
                continue
            if case.get("fresh"):
                n2, w2 = make_graph()
                run_case(n2, w2, reads, case, mf)
            else:
                run_case(node, w, reads, case, mf)
    print("SWEEP_RENDER_DONE", flush=True)
    with open(MANIFEST) as f:
        records = [json.loads(line) for line in f if line.strip()]
    report(records)


main()
quit()
