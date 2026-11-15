#!/usr/bin/env python3
"""Sequential, correctness-gated speed suite. See README.md for the protocol."""
import argparse
import datetime
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import re
import shutil
import signal
import statistics
import subprocess
import sys
import tomllib

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
LOAD = ROOT / "scripts/load"
sys.path.insert(0, str(LOAD))
import run as R  # noqa: E402
import measure as M  # noqa: E402

# Matrix aliases here are stable result identifiers, not moving image tags.
ENGINE_ORDER = [f"{e}-{v}" for e in ("es710", "es717", "elasticsearch8", "es819", "elasticsearch",
                                    "os13", "os2", "os3") for v in ("plain", "secure")]
class TeardownError(RuntimeError):
    pass


class VerifyMismatch(RuntimeError):
    """edverify ran and reported different document sets (exit 1), as opposed to an error (exit 2)."""


PROTOCOL = "edrs-perf/v1"
LIMIT = 2000  # existing dump-restore-logs scenario; identical for Node and Rust


def cases(ab=False):
    """The case manifest. It is part of the baseline identity, so the default list is frozen
    for comparability with recorded baselines (v1.0.0-beta1). `ab=True` (PERF_AB_CASES=1)
    appends the 1.1 A/B cases, which changes the identity: use them with their own
    PERF_HOST_ID / baseline directory."""
    rows = []

    def add(id_, kind, dataset="logs", codec="none", flags=(), node=True, **extra):
        rows.append(dict(id=id_, kind=kind, dataset=dataset, codec=codec, flags=list(flags), node=node,
                         limit=10 if dataset == "big" else LIMIT, **extra))

    for codec in ("none", "gzip", "zstd"):
        for kind in ("dump", "restore"):
            add(f"{kind}-{codec}", kind, codec=codec, node=codec != "zstd")
    add("copy", "copy")
    for s in (1, 2, 4, 8):
        add(f"dump-scroll-s{s}", "dump", flags=[f"--slices={s}"], node=False)
        add(f"dump-pit-s{s}", "dump", flags=["--searchAfter", "--pit", f"--slices={s}"], node=False, pit=True)
    for codec in ("none", "gzip", "zstd"):
        for kind in ("dump", "restore"):
            add(f"{kind}-split-{codec}", kind, codec=codec, split=True, node=codec != "zstd")
    for ds in ("small", "big", "edge"):
        for kind in ("dump", "restore"):
            add(f"{kind}-{ds}", kind, dataset=ds)
    for t in ("mapping", "settings"):
        for kind in ("dump", "restore"):
            add(f"{t}-{kind}", kind, meta=t)
    for p in (1, 4, 8):
        for kind in ("dump", "restore"):
            add(f"multi-{kind}-p{p}", kind, dataset="many", parallel=p)
    if ab:
        # 1.1-1 upper bound: no refresh at all (both tools support --noRefresh), to compare the
        # default interval refresh and Node's per-batch refresh (restore-none, copy) against.
        add("restore-norefresh", "restore", flags=["--noRefresh"])
        add("copy-norefresh", "copy", flags=["--noRefresh"])
    return rows


CASES = cases(ab=os.environ.get("PERF_AB_CASES") == "1")


def write_json(path, data):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    # Baselines and completed reports must not become partially visible.
    tmp = path.with_suffix(path.suffix + ".tmp")
    tmp.write_text(json.dumps(data, indent=2) + "\n")
    tmp.replace(path)


class ToolTimeout(RuntimeError):
    """A measured command exceeded its wall-clock limit and was killed (whole process group)."""


# Per-command wall-clock limit for timed tool runs and verifications. L0 cases take seconds to
# a few minutes; a run past this is hung or looping (or its engine died) and must not stall
# the rest of the sequence. PERF_CMD_TIMEOUT_S overrides it.
CMD_TIMEOUT_S = int(os.environ.get("PERF_CMD_TIMEOUT_S", "1800"))


def checked(cmd, timeout_s=None, **kw):
    if timeout_s is None:
        return subprocess.run([str(x) for x in cmd], check=True, **kw)
    # Own session so the kill reaches measure.py AND the tool it wraps (and Node's forked children).
    p = subprocess.Popen([str(x) for x in cmd], start_new_session=True, **kw)
    try:
        code = p.wait(timeout=timeout_s)
    except subprocess.TimeoutExpired:
        for sig in (signal.SIGTERM, signal.SIGKILL):
            try:
                os.killpg(p.pid, sig)
            except ProcessLookupError:
                break
            try:
                p.wait(timeout=10)
                break
            except subprocess.TimeoutExpired:
                continue
        raise ToolTimeout(f"timed out after {timeout_s}s and was killed: {' '.join(str(x) for x in cmd[-12:])}")
    except BaseException:
        # Interrupted suite (SIGTERM/^C): do not leave the measured tool running.
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        raise
    if code:
        raise subprocess.CalledProcessError(code, cmd)
    return subprocess.CompletedProcess(cmd, code)


def capture(cmd, **kw):
    return checked(cmd, capture_output=True, text=True, **kw).stdout.strip()


def select(value, allowed, label):
    picked = value.split(",") if value else list(allowed)
    unknown = set(picked) - set(allowed)
    if unknown or len(set(picked)) != len(picked):
        raise ValueError(f"invalid {label}: {sorted(unknown) or 'duplicate entries'}")
    # Always follow the documented sequence even when the selection is reordered.
    return [x for x in allowed if x in picked]


def engine_env(engine):
    p = engine
    env = dict(os.environ)
    # Do not let ambient image overrides silently defeat the pinned suite.
    for k in ("ES_IMAGE", "ES_TAG", "OS_IMAGE", "OS_TAG", "IT_UNPINNED", "IT_POD", "IT_ES_DATA_MOUNT",
              "ES717_IMAGE", "ES710_PLAIN_IMAGE", "ES710_SECURE_IMAGE", "ES819_IMAGE", "ES8_TAG", "OS13_TAG"):
        env.pop(k, None)
    if engine.startswith(("os2-", "os3-")):
        env["OS_TAG"] = engine[2]
        p = "opensearch-" + engine.rsplit("-", 1)[1]
    values = dict(line.split("=", 1) for line in capture([ROOT / "scripts/it/matrix.sh", p], env=env).splitlines())
    profile = values.pop("UP_PROFILE")
    values.setdefault("OS_TAG", env.get("OS_TAG", "2"))
    values.setdefault("ES_TAG", "9.1.2-debian-12-r0")
    values.setdefault("ES_IMAGE", "docker.io/bitnamilegacy/elasticsearch:" + values["ES_TAG"])
    values.setdefault("OS_IMAGE", "docker.io/opensearchproject/opensearch:" + values["OS_TAG"])
    lock = dict(line.split() for line in (ROOT / "scripts/it/images.lock").read_text().splitlines()
                if line.strip() and not line.startswith("#"))
    for k in ("ES_IMAGE", "OS_IMAGE"):
        if values[k] not in lock:
            raise ValueError(f"unpinned matrix image: {values[k]}")
        values[k] += "@" + lock[values[k]]
    env.update(values)
    env.update(IT_REQUIRE_PIN="1", IT_ES_DATA_VOLUME="1")
    return profile, env


def skip_reason(engine, case, tool):
    if case.get("pit") and (engine.startswith("es710-") or engine.startswith("os13-")):
        return "conservative cross-distribution PIT exclusion (ES 7.10 / OS 1.3)"
    if tool == "node" and not case["node"]:
        return "Node 6.125.1 has no equivalent zstd/slices/PIT case"
    return None


def engine_heap_mb(engine, tier_heap_mb):
    """Per-engine heap. OpenSearch 2/3 still need a modest raise after query-insights is
    disabled in up.sh: L0 restore-big keeps source+dest huge-doc indices resident and
    trips the 1g parent breaker (HTTP 429 even on DELETE /_search/scroll/_all). 2048 MB
    still tripped it while reading back big; 4096 MB is the current setting; other
    engines keep the tier heap."""
    if engine.startswith(("os2-", "os3-")):
        return max(tier_heap_mb, 4096)
    return tier_heap_mb


def make_plan(a):
    tiers = tomllib.loads((LOAD / "scenarios/tiers.toml").read_text())
    if a.place not in tiers["place"]:
        raise ValueError(f"unknown place {a.place}")
    original = tomllib.loads((LOAD / "scenarios/dump-restore-logs.toml").read_text())
    scenario = dict(original, seed=[original["seed"][0]], step=[{"kind": "dump"}] * 2 + [{"kind": "restore"}])
    ok, why, fit = R.fit(a.tier, a.place, tiers, scenario, None)
    if not ok:
        raise ValueError(why)
    n = tiers["tier"][a.tier]["docs"]
    # Approximately equal raw bytes for logs and huge; do NOT seed tier-count multi-MB docs.
    sizes = dict(logs=n, small=n, big=max(10, n // 3000), edge=tiers["tier"][a.tier]["edge_docs"], many=n)
    # Four raw-equivalent copies plus engine index overhead, bounded by dataset-at-a-time cleanup.
    disk = max(fit["need_disk_mb"], math.ceil(n * 1000 * 6 / 1e6) + 1024)
    if disk > fit["place"]["max_disk_mb"]:
        raise ValueError(f"suite disk estimate {disk} MB exceeds {a.place} budget; use LOAD_PLACE=big")
    engines = select(a.engines, ENGINE_ORDER, "engines")
    selected = select(a.cases, [c["id"] for c in CASES], "cases")
    cs = [c for c in CASES if c["id"] in selected]
    result = dict(protocol=PROTOCOL, tier=a.tier, place=a.place, sizes=sizes, limit=LIMIT,
                  heap_mb=fit["heap_mb"], need_disk_mb=disk, place_config=fit["place"], reps=a.reps,
                  warmups=1, threshold_pct=a.threshold, engine_order=engines, engines=[])
    for e in engines:
        profile, env = engine_env(e)
        image = env["OS_IMAGE"] if profile.startswith("opensearch") else env["ES_IMAGE"]
        result["engines"].append(dict(engine=e, profile=profile, image=image,
                                        heap_mb=engine_heap_mb(e, fit["heap_mb"]), cases=[
            dict(c, tools={t: skip_reason(e, c, t) or "RUN" for t in (["rs"] if a.skip_node else ["rs", "node"])})
            for c in cs]))
    return result


def aggregate(metrics):
    """Only callers with all repetitions correct may aggregate; warm-up never enters here."""
    if not metrics or any(m.get("exit_code") or m.get("failed_writes") for m in metrics):
        raise ValueError("cannot aggregate empty or failed metrics")
    out = {}
    for k in ("wall_s", "user_s", "sys_s", "cpu_s", "max_rss_kb", "fds_peak", "docs_per_s", "mb_per_s_in",
              "mb_per_s_out", "logical_mb_per_s", "compression_ratio", "input_bytes", "output_bytes"):
        vs = [m[k] for m in metrics if m.get(k) is not None]
        out[k] = statistics.median(vs) if vs else None
    w = sorted(m["wall_s"] for m in metrics)
    out.update(wall_p90_s=w[math.ceil(.9 * len(w)) - 1], wall_min_s=w[0], wall_max_s=w[-1],
               wall_cv_pct=100 * statistics.pstdev(w) / statistics.mean(w), n=len(w),
               exit_code=0, failed_writes=0)
    return out


def compare_metrics(base, new, threshold, directory):
    """Reuse the established regression exit code and metric directions, not a second comparator."""
    directory = Path(directory)
    write_json(directory / "base.json", base)
    write_json(directory / "new.json", new)
    p = subprocess.run([sys.executable, str(LOAD / "compare.py"), str(directory / "base.json"),
                        str(directory / "new.json"), "--threshold", str(threshold)], capture_output=True, text=True)
    (directory / "compare.txt").write_text(p.stdout + p.stderr)
    if p.returncode not in (0, 1):
        raise RuntimeError("baseline comparator failed: " + p.stderr)
    return p.returncode == 1


def row_summary(case, results, base, threshold, directory):
    rs, node = results.get("rs"), results.get("node")
    row = dict(case=case, rs=rs, node=node, rs_over_node=None, baseline_delta_pct=None, status="FAIL")
    if rs is None:
        return row
    if node is not None:
        row["rs_over_node"] = rs["wall_s"] / node["wall_s"]
    row["status"] = "PASS (NO_BASELINE)" if base is None else "PASS"
    if base:
        row["baseline_delta_pct"] = (rs["wall_s"] / base["wall_s"] - 1) * 100
        if compare_metrics(base, rs, threshold, directory):
            row["status"] = "REGRESSION"
    row["node_comparison"] = "SLOWER" if row["rs_over_node"] and row["rs_over_node"] > 1 else "informational"
    return row


def markdown(results):
    engines = results.get("engines") or []
    names = [e.get("engine", "?") for e in engines]
    title = "searchdump sequential speed suite"
    if len(names) == 1:
        title += f" — {names[0]}"
        ver = engines[0].get("engine_version")
        if ver:
            title += f" ({ver})"
    lines = [f"# {title}", ""]

    def wall(m):
        return None if not m or m.get("wall_s") is None else m["wall_s"]

    def fmt_s(v, flag_low=False):
        if v is None:
            return "—"
        mark = " †" if flag_low and v < 0.5 else ""
        return f"{v:.3f}{mark}"

    def fmt_x(rs_s, node_s):
        if rs_s and node_s:
            return f"{node_s / rs_s:.2f}×"
        return "—"

    def fmt_docs(m):
        if not m or m.get("docs_per_s") is None:
            return "—"
        return f"{m['docs_per_s']:.0f}"

    def fmt_rss(m):
        if not m or m.get("max_rss_kb") is None:
            return "—"
        return f"{m['max_rss_kb'] / 1024:.1f}"

    def fmt_delta(v):
        return "—" if v is None else f"{v:.3f}%"

    n_pass = n_fail = n_skip = n_base = 0
    ratios = []
    low = False
    for engine in engines:
        for row in engine.get("summary") or []:
            st = row.get("status") or ""
            if "NO_BASELINE" in st:
                n_base += 1
            if st.startswith("PASS"):
                n_pass += 1
            elif st.startswith("SKIP") or st.startswith("NOT_RUN") or st.startswith("REFUSED"):
                n_skip += 1
            else:
                n_fail += 1
            rs_s, nd_s = wall(row.get("rs")), wall(row.get("node"))
            if rs_s and nd_s:
                ratios.append(nd_s / rs_s)
            if (rs_s is not None and rs_s < 0.5) or (nd_s is not None and nd_s < 0.5):
                low = True

    head = f"**Results:** {n_pass} PASS, {n_fail} FAIL, {n_skip} skipped/not-run"
    if n_base:
        head += f", {n_base} NO_BASELINE"
    if ratios:
        head += f"; × faster median {statistics.median(ratios):.2f}× over {len(ratios)} Node-compared cases"
    lines += [head, ""]

    meta = []
    if results.get("git"):
        meta.append(f"git `{results['git']}`")
    if results.get("started"):
        finished = results.get("finished") or "in progress"
        meta.append(f"started {results['started']}, finished {finished}")
    plan = results.get("plan") or {}
    bits = []
    if plan.get("tier"):
        bits.append(f"tier {plan['tier']}")
    if plan.get("limit") is not None:
        bits.append(f"limit {plan['limit']}")
    if plan.get("heap_mb") is not None:
        bits.append(f"heap {plan['heap_mb']} MB")
    if plan.get("reps") is not None:
        bits.append(f"{plan['reps']} reps")
    if bits:
        meta.append(", ".join(bits))
    hashes = results.get("tool_hashes") or {}
    for key in ("searchdump", "elasticdump-rs", "searchdump-multi", "multielasticdump"):
        if key in hashes:
            meta.append(f"{key} sha256 `{hashes[key][:12]}…`")
    if results.get("node_runtime"):
        meta.append(f"Node {results['node_runtime']}")
    if meta:
        lines += ["- " + x for x in meta] + [""]

    lines += [
        'Legend: "× faster" is Node wall / searchdump wall (higher means searchdump is faster).',
        "NODE_DIFF = Node's output differs in value from the source (e.g. integers above 2^53);",
        "NODE_FAIL = Node errored; NODE_TIMEOUT = Node exceeded the per-command limit and was killed.",
        "All three are recorded for comparison only and never fail a searchdump case.",
        "Node SLOWER is a warning, never a baseline failure. Missing baselines are not regression-tested.",
    ]
    if low:
        lines.append(
            "† Wall time is below 0.5 s (around the 0.1 s timer resolution); treat that ratio as indicative only."
        )
    lines.append("")

    setup_prefixes = ("FAIL: setup", "REFUSED:", "NOT_RUN:", "FAIL: orchestration")

    def is_setup(row):
        st = row.get("status") or ""
        return any(st.startswith(p) for p in setup_prefixes)

    for engine in engines:
        heading = f"## searchdump × {engine.get('engine', '?')}"
        if engine.get("engine_version"):
            heading += f" ({engine['engine_version']})"
        lines += [heading, ""]
        summary = engine.get("summary") or []
        if summary and all(is_setup(row) for row in summary):
            st = summary[0].get("status", "setup failure")
            lines += [f"**Setup failure:** {len(summary)} cases not run (`{st}`). Collapsed; they share the same cause.", ""]
            for error in engine.get("failures", []):
                lines.append("Failure: " + str(error).replace("\n", " ").replace("|", "/"))
            if engine.get("failures"):
                lines.append("")
            continue
        lines += [
            "| Case | searchdump s | Node s | × faster | searchdump docs/s | peak RSS MB | Baseline wall delta | Status |",
            "|---|---:|---:|---:|---:|---:|---:|---|",
        ]
        for row in summary:
            rs, nd = row.get("rs") or {}, row.get("node") or {}
            rs_s, nd_s = wall(rs), wall(nd)
            status = row.get("status") or ""
            if row.get("node_status"):
                status = f"{status}; {row['node_status']}"
            lines.append(
                f"| {row['case']} | {fmt_s(rs_s, True)} | {fmt_s(nd_s, True)} | "
                f"{fmt_x(rs_s, nd_s)} | {fmt_docs(rs)} | {fmt_rss(rs)} | "
                f"{fmt_delta(row.get('baseline_delta_pct'))} | {status} |"
            )
        for error in engine.get("failures", []):
            lines.append("\nFailure: " + str(error).replace("\n", " ").replace("|", "/"))
        for note in engine.get("node_differences", []):
            lines.append("\nNode (informational, not an rs failure): " + str(note).replace("\n", " ").replace("|", "/"))
        lines.append("")
    return "\n".join(lines) + "\n"


def prerequisites(a, plan):
    for cmd in ("podman", "bash", "curl", "openssl", "git"):
        if not shutil.which(cmd):
            raise ValueError(f"missing prerequisite {cmd}; use run-suite.sh controller")
    # Check ALL lock entries: helpers are not engines, but belong to the previous suite's image set.
    missing = []
    for line in (ROOT / "scripts/it/images.lock").read_text().splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        tag, digest = line.split()
        if subprocess.run(["podman", "image", "exists", tag + "@" + digest]).returncode:
            missing.append(tag + "@" + digest)
    if missing:
        raise ValueError("Pinned images missing; pull them explicitly before testing:\n" + "\n".join(missing))
    if capture(["podman", "ps", "-aq", "--filter", "label=searchdump-it=1"]):
        raise ValueError("existing IT containers found; stop their owning run before starting (nothing removed)")
    bindir = Path(os.environ.get("PERF_BIN_DIR", ROOT / "results/perf/build/release"))
    binaries = ("searchdump", "searchdump-multi", "edgen", "edverify")
    if "PERF_BIN_DIR" in os.environ:
        if any(not os.access(bindir / b, os.X_OK) for b in binaries):
            raise ValueError("PERF_BIN_DIR must contain all four release binaries")
    else:
        if "PERF_CONTROLLER_ID" not in os.environ:
            raise ValueError("Build in the controller via run-suite.sh, or set PERF_BIN_DIR")
        # Run Cargo even when binaries exist, so source changes cannot benchmark stale executables.
        target = bindir.parent
        checked(["cargo", "+1.91.1", "build", "--locked", "--release", "--workspace", "--bins",
                 "--jobs", os.environ.get("PERF_BUILD_JOBS", "2"), "--target-dir", target], cwd=ROOT)
    tools = {b: str(bindir / b) for b in binaries}
    if not a.skip_node:
        node_dir = Path(os.environ.get("NODE_ED_BIN", "/opt/node-ed/node_modules/.bin"))
        for b in ("elasticdump", "multielasticdump"):
            if not os.access(node_dir / b, os.X_OK):
                raise ValueError(f"missing Node executable {node_dir / b}")
            tools["node-" + b] = str(node_dir / b)
        if capture([tools["node-elasticdump"], "--version"]).splitlines()[-1] != "6.125.1":
            raise ValueError("Node elasticdump must be exactly 6.125.1")
    return tools


def codec_flags(codec):
    return {"none": [], "gzip": ["--fsCompress", "--compressionLevel=6"],
            "zstd": ["--compressionType=zstd", "--compressionLevel=3"]}[codec]


def meta_value(cl, index, kind):
    endpoint = "mapping" if kind == "mappings" else kind
    value = cl.req("GET", f"/{index}/_{endpoint}")[index][kind]
    if kind == "settings":
        value = dict(value["index"])
        for k in ("uuid", "version", "creation_date", "provided_name", "history_uuid"):
            value.pop(k, None)
    return value


def node_auth_ini(user, pw):
    """Node elasticdump --httpAuthFile content (ini `user=` / `password=`; see lib/add-auth.js)."""
    return f"user={json.dumps(user)}\npassword={json.dumps(pw)}\n"


def engine_port():
    """Loopback port the engine is published on (PERF_ENGINE_PORT, default 19222)."""
    v = os.environ.get("PERF_ENGINE_PORT", "19222")
    if not v.isdigit() or not 1 <= int(v) <= 65535:
        raise ValueError(f"PERF_ENGINE_PORT must be a TCP port, got {v!r}")
    return int(v)


def port_in_use(port, host="127.0.0.1"):
    """True when something already listens on host:port (a leftover engine makes every case fail)."""
    import socket
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.settimeout(0.5)
        return s.connect_ex((host, port)) == 0


class EngineRun:
    def __init__(self, a, plan, engine, tools, directory):
        self.a, self.plan, self.entry, self.tools, self.directory = a, plan, engine, tools, directory
        self.work = directory / "work"
        self.work.mkdir(mode=0o700)
        profile, self.env = engine_env(engine["engine"])
        self.profile = profile
        # Prefer the per-engine heap when set; fall back to the tier default.
        self.heap_mb = engine.get("heap_mb", plan["heap_mb"])
        # Cgroup limit must cover heap + off-heap; place.box's 6g is tight for a 4g heap.
        place_cmem = plan["place_config"]["container_mem"]
        need_cmem_mb = self.heap_mb * 2 + 512
        # place_cmem is like "6g" / "1536m"; compare in MB without importing guards.sh.
        _u = place_cmem.strip().lower()
        _place_mb = int(float(_u[:-1]) * (1024 if _u.endswith("g") else 1)) if _u[-1] in "gm" else int(_u)
        cmem = place_cmem if _place_mb >= need_cmem_mb else f"{need_cmem_mb}m"
        self.env.update(IT_DIR=str(self.work / "it"), IT_LOCK_HELD="1", RUN_ID=f"perf-{os.getpid()}-{engine['engine']}",
                        ES_HEAP=f"{self.heap_mb}m", OS_HEAP=f"{self.heap_mb}m", PORT=str(engine_port()),
                        IT_CONTAINER_MEM=cmem, IT_CONTAINER_CPUS=str(os.cpu_count()))
        self.result = dict(engine=engine["engine"], image=engine["image"], runs=[], summary=[], failures=[], seeds=[])
        self.cl = None

    def start(self):
        port = engine_port()
        if port_in_use(port):
            raise RuntimeError(f"engine port 127.0.0.1:{port} is already in use by another process; "
                               "stop it or set PERF_ENGINE_PORT to a free port")
        checked([ROOT / "scripts/it/up.sh", self.profile], env=self.env, stdout=subprocess.DEVNULL)
        ce = R.sh_env(str(self.work / "it/env"))
        self.url = ce["IT_ES_URL"]
        user, pw = ce.get("IT_ADMIN_USER"), ce.get("IT_ADMIN_PW")
        self.cl = R.Cluster(self.url, user, pw, ce.get("IT_CA"))
        self.tenv = dict(os.environ)
        self.rs_tls, self.node_tls, self.edv_tls = [], [], []
        if user:
            self.tenv.update(EDV_USER=user, EDV_PASSWORD=pw,
                             SEARCHDUMP_INPUT_USERNAME=user, SEARCHDUMP_INPUT_PASSWORD=pw,
                             SEARCHDUMP_OUTPUT_USERNAME=user, SEARCHDUMP_OUTPUT_PASSWORD=pw)
            # Node parent's discovery does not use child transport auth env. Keep auth OUT of argv/logs.
            # Node reads --httpAuthFile with the `ini` parser (lib/add-auth.js) and throws
            # "Malformed Auth File" unless both `user` and `password` keys are present. Values are
            # JSON-quoted, which `ini` unquotes, so ';', '#', '=' in a password survive.
            auth = self.work / "node-auth.ini"
            fd = os.open(auth, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            with os.fdopen(fd, "w") as f:
                f.write(node_auth_ini(user, pw))
            self.node_tls = ["--httpAuthFile=" + str(auth)]
        if self.url.startswith("https"):
            # Test-only PKI; use the same verification policy for both tools and parents.
            self.rs_tls = ["--insecure"]
            self.tenv["NODE_TLS_REJECT_UNAUTHORIZED"] = "0"
            self.edv_tls = ["--insecure"]
        self.result["engine_version"] = self.cl.req("GET", "/")["version"]["number"]

    def verify(self, tag, source, targets, tool="rs"):
        report = self.directory / ("verify-" + tag + ".json")
        # Node re-serialises every document through JSON.parse/stringify, which rewrites number
        # spelling (79.746790 -> 79.74679, 1.0 -> 1, 1e308 -> 1e+308) without changing the value.
        # Its outputs are compared by exact decimal value; a changed value (integers above 2^53,
        # long mantissas) still fails. searchdump keeps bytes, so it keeps the strict check.
        numbers = ["--decimal-numbers"] if tool == "node" else []
        # Bounded scroll pages for large documents: edverify's default (4 slices x 1000 docs) asks
        # the engine for ~150 MB (edge: 1 MB docs, 10k arrays) or every 3-10 MB `big` doc at once,
        # which tripped the 1 GB-heap parent circuit breaker (HTTP 429) on restore-edge.
        page = {"big": ["--size=2", "--slices=2"], "edge": ["--size=100"]}.get(getattr(self, "dataset", None), [])
        cmd = [self.tools["edverify"], "diff", "--canonical", *numbers, *page, "--index-as", "x", "--a", source,
               "--b", *targets, "--out", str(report), *self.edv_tls]
        try:
            checked(cmd, env=self.tenv, stdout=subprocess.DEVNULL, timeout_s=CMD_TIMEOUT_S)
        except subprocess.CalledProcessError as e:
            if e.returncode == 1:
                raise VerifyMismatch(f"edverify diff {tag}: document sets differ (report {report.name})") from None
            raise

    def plain(self, args):
        checked([self.tools["searchdump"], *args, *self.rs_tls], env=self.tenv, stdout=subprocess.DEVNULL)

    def seed(self, dataset):
        n = self.plan["sizes"][dataset]
        self.dataset = dataset
        profile = {"logs": "logs", "small": "geo", "big": "huge", "edge": "edge", "many": "logs"}[dataset]
        specs = [dict(index=f"perf-src-{i:02d}", profile=profile, docs=n // 16 + (i < n % 16), seed=1)
                 for i in range(16)] if dataset == "many" else [dict(index="perf-src", profile=profile, docs=n, seed=1)]
        self.sources = []
        for spec in specs:
            rec = R.seed_index(self.cl, self.tools["edgen"], spec, R.engine_mapping_flavour(self.entry["engine"]))
            if not rec["stored"]:
                raise RuntimeError("seed stored no documents")
            if dataset != "edge" and (rec["rejected"] or rec["overwritten"]):
                raise RuntimeError("non-edge seed has rejections/overwrites")
            self.result["seeds"].append(rec)
            self.sources.append(rec)
        self.refdir = self.work / "refs"
        self.refdir.mkdir()
        self.refs = {}
        for rec in self.sources:
            idx = rec["index"]
            p = self.refdir / (idx + ".json")
            self.plain([f"--input={self.url}/{idx}", f"--output={p}", f"--limit={10 if dataset == 'big' else LIMIT}"])
            self.verify("seed-" + idx + "-" + dataset, f"{self.url}/{idx}", [str(p)])
            if dataset == "many":
                # Nonempty mapping marker is needed for Rust's manifest-free Node-name discovery.
                self.plain([f"--input={self.url}/{idx}", f"--output={self.refdir / (idx + '.mapping.json')}",
                            "--type=mapping"])
        self.raw_bytes = sum(p.stat().st_size for p in self.refdir.iterdir() if not p.name.endswith(".mapping.json"))
        self.count = sum(r["stored"] for r in self.sources)

    def reference(self, case):
        if case["dataset"] == "many":
            return self.refdir
        if case.get("meta"):
            key = case["meta"]
        else:
            key = case["codec"] + ("-split" if case.get("split") else "")
        if key not in self.refs:
            d = self.work / ("ref-" + key)
            d.mkdir()
            out = d / ("data.json" + {"none": "", "gzip": ".gz", "zstd": ".zst"}[case["codec"]])
            flags = ["--type=" + case.get("meta", "data")] + codec_flags(case["codec"])
            if case.get("split"):
                flags += ["--fileSize=20mb"]
            self.plain([f"--input={self.url}/perf-src", f"--output={out}", f"--limit={case['limit']}", *flags])
            paths = sorted(d.iterdir())
            if not case.get("meta"):
                self.verify("reference-" + key, self.url + "/perf-src", [str(p) for p in paths])
            self.refs[key] = paths
        return self.refs[key]

    def create_target(self, source, target, meta=False):
        self.cl.req("DELETE", "/" + target, ok404=True)
        if meta == "settings":
            # Both tools apply index settings by CREATING the index (Node: PUT /<index> with the
            # settings body; searchdump refuses an existing index because static settings such
            # as number_of_shards cannot be changed). A pre-created target made rs exit 1 and let
            # the metadata check compare two harness-made indices; leave the target absent.
            return
        if meta:
            self.cl.req("PUT", "/" + target, {"settings": {"number_of_shards": 1, "number_of_replicas": 0}})
        else:
            self.cl.req("PUT", "/" + target, dict(settings={"index": meta_value(self.cl, source, "settings")},
                                                 mappings=meta_value(self.cl, source, "mappings")))

    def iteration(self, case, tool, rep):
        tag = f"{case['id']}-{tool}-{'warmup' if rep < 0 else rep}"
        outdir = self.work / "output"
        outdir.mkdir()
        meta = case.get("meta")
        typ = meta or "data"
        flags = [f"--limit={case['limit']}", "--concurrency=1", *case["flags"], *codec_flags(case["codec"])]
        paths, inputs, targets = [], [], []
        multi = case["dataset"] == "many"
        docs = len(self.sources) if meta else self.count
        if multi:
            exe = self.tools["searchdump-multi"] if tool == "rs" else self.tools["node-multielasticdump"]
        else:
            exe = self.tools["searchdump"] if tool == "rs" else self.tools["node-elasticdump"]
        tls = self.rs_tls if tool == "rs" else self.node_tls
        output = None
        if multi:
            child_flags = flags
            flags = ["--includeType=data", "--interval=0", f"--parallel={case['parallel']}"]
            if case["kind"] == "dump":
                inp, output = self.url, str(outdir / "multi")
                if tool == "node":
                    # Node's multielasticdump exits 1 with "Directory --output ... does not exists".
                    Path(output).mkdir()
                if tool == "rs":
                    flags += ["--nodeCompat"]
                flags += ["--match=^perf-src-"]
                # Node 6.125.1 knows only matchType alias|datastream ("invalid matchType index", exit 1);
                # its default `alias` lists /_aliases and tests the INDEX name as well as alias names.
                # searchdump names that selection `index` (its default; its `alias` = alias names only).
                flags += ["--matchType=index"] if tool == "rs" else []
            else:
                inp, output = str(self.refdir), self.url
                inputs = [str(self.refdir)]
                flags += ["--prefix=perf-dst-"]
                for rec in self.sources:
                    target = "perf-dst-" + rec["index"]
                    self.cl.req("DELETE", "/" + target, ok404=True)
                    targets.append(target)
            if tool == "rs":
                flags += ["--compression=none", "--noDefaultDataFlags", "--logDir=" + str(self.directory / (tag + "-children"))]
                cmd = [exe, "--direction=" + ("load" if case["kind"] == "restore" else "dump"),
                       "--input=" + inp, "--output=" + output, *flags, *tls, "--", *child_flags]
            else:
                cmd = [exe, "--direction=" + ("load" if case["kind"] == "restore" else "dump"),
                       "--input=" + inp, "--output=" + output, *flags, *child_flags, *tls]
        else:
            if case["kind"] == "dump":
                inp = self.url + "/perf-src"
                output = str(outdir / ("data.json" + {"none": "", "gzip": ".gz", "zstd": ".zst"}[case["codec"]]))
                if case.get("split"):
                    flags += ["--fileSize=20mb"]
            else:
                self.create_target("perf-src", "perf-dst", meta=meta)
                targets = ["perf-dst"]
                output = self.url + "/perf-dst"
                if case["kind"] == "copy":
                    inp = self.url + "/perf-src"
                else:
                    paths = self.reference(case)
                    inputs = [str(p) for p in paths]
                    inp = str(paths[0])
            flags += ["--type=" + typ]
            cmd = [exe, "--input=" + inp, "--output=" + output, *flags, *tls]
            if case.get("split") and case["kind"] == "restore":
                # One measured job, same subprocess loop for both tools, numeric part order.
                paths = sorted(paths, key=lambda p: int(p.name.split(".split-")[1].split(".")[0]))
                cmd = [sys.executable, str(HERE / "parts.py"), "--files", *map(str, paths), "--",
                       exe, "--output=" + output, *flags, *tls]
        metric_path = self.directory / (tag + ".json")
        measure = [str(LOAD / "measure.sh"), "--out", str(metric_path), "--docs", str(docs),
                   "--cluster", self.url, "--label", "case=" + case["id"], "--label", "tool=" + tool,
                   "--uncompressed-bytes", str(self.raw_bytes)]
        if case["kind"] == "dump":
            measure += ["--output", output if multi else str(outdir)]
        for path in inputs:
            measure += ["--input", path]
        row = dict(case=case["id"], tool=tool, rep=rep, correct=False, metrics=str(metric_path.relative_to(ROOT)))
        self.result["runs"].append(row)
        try:
            with (self.directory / (tag + ".stdout")).open("w") as stdout:
                checked(measure + ["--", *cmd], env=self.tenv, stdout=stdout, timeout_s=CMD_TIMEOUT_S)
            m = json.loads(metric_path.read_text())
            errs = M.validate(m)
            rss_cap_kb = self.plan["place_config"]["tool_mem_mb"] * 1024
            if errs or m["failed_writes"] or m["max_rss_kb"] > rss_cap_kb:
                raise RuntimeError(f"metrics/write/RSS gate: schema errors={errs} failed_writes={m['failed_writes']} "
                                   f"max_rss_kb={m['max_rss_kb']} (cap {rss_cap_kb})")
            if not multi and not meta and m["total_writes"] is not None and not case.get("split") and m["total_writes"] != self.count:
                raise RuntimeError("write count differs from stored source count")
            if case["kind"] == "dump":
                # Some Node transports gzip at exactly the requested path; detect by magic, not assumptions.
                for p in list(outdir.rglob("*")):
                    if p.is_file():
                        with p.open("rb") as f:
                            gz = f.read(2) == b"\x1f\x8b"
                        if gz and not p.name.endswith(".gz"):
                            p.rename(str(p) + ".gz")
                if multi:
                    for rec in self.sources:
                        p = Path(output) / (rec["index"] + ".json")
                        self.verify(tag + "-" + rec["index"], self.url + "/" + rec["index"], [str(p)], tool)
                else:
                    paths = sorted(str(p) for p in outdir.iterdir() if p.is_file())
                    if not paths:
                        raise RuntimeError("no dump files")
                    if meta:
                        # Round-trip the actual timed output, then verify normalized metadata with edverify.
                        self.create_target("perf-src", "perf-dst", meta=meta)
                        targets = ["perf-dst"]
                        self.plain(["--input=" + paths[0], "--output=" + self.url + "/perf-dst", "--type=" + meta])
                    else:
                        self.verify(tag, self.url + "/perf-src", paths, tool)
            else:
                for rec, target in zip(self.sources, targets):
                    self.cl.req("POST", f"/{target}/_refresh")
                    if not meta:
                        self.verify(tag + "-" + target, self.url + "/" + rec["index"], [self.url + "/" + target], tool)
            if meta:
                key = "mappings" if meta == "mapping" else "settings"
                pair = []
                for idx in ("perf-src", "perf-dst"):
                    p = self.work / (idx + "-meta.json")
                    write_json(p, dict(_index="x", _id="metadata", _source=meta_value(self.cl, idx, key)))
                    # edverify needs one NDJSON document, not pretty JSON.
                    p.write_text(json.dumps(json.loads(p.read_text())) + "\n")
                    pair.append(str(p))
                self.verify(tag + "-metadata", pair[0], [pair[1]], tool)
                for p in pair:
                    Path(p).unlink()
            if not meta:
                m["logical_mb_per_s"] = self.raw_bytes / 1e6 / m["wall_s"]
                write_json(metric_path, m)
            # Full digest gates ABOVE must pass before the sample can count.
            row.update(correct=True, values=m)
            if tool == "rs":
                _, leaks = R.leak_check(self.cl, wait_s=15)
                # Node may leave scrolls; clear them at each iteration below so Rust is isolated.
                if leaks:
                    raise RuntimeError("; ".join(leaks))
            return m
        except Exception as e:
            row.update(correct=False, error=str(e))
            raise
        finally:
            # Scope is a private test engine, not a user cluster. Prevent Node scroll accumulation.
            # Best effort: an unreachable engine must not skip the output-dir removal, or every
            # later iteration fails with "File exists" instead of the real error.
            for path in ["/_search/scroll/_all"] + ["/" + t for t in targets]:
                try:
                    if self.cl:
                        self.cl.req("DELETE", path, ok404=True)
                except Exception as e:
                    self.result["failures"].append(f"{tag}: cleanup DELETE {path}: {e}")
                    break
            shutil.rmtree(outdir, ignore_errors=True)

    def cleanup_dataset(self):
        for rec in getattr(self, "sources", []):
            self.cl.req("DELETE", "/" + rec["index"], ok404=True)
        for p in self.work.glob("ref*"):
            shutil.rmtree(p)

    def close(self):
        p = subprocess.run([str(ROOT / "scripts/it/down.sh")], env=self.env)
        if p.returncode:
            raise TeardownError("engine teardown failed; stopping sequence to preserve one-engine invariant")
        shutil.rmtree(self.work, ignore_errors=True)


def execute_engine(a, plan, entry, tools, directory):
    er = EngineRun(a, plan, entry, tools, directory)
    identity = dict(protocol=PROTOCOL, host=os.environ.get("PERF_HOST_ID", platform.node()), tier=a.tier,
                    place=a.place, image=entry["image"],
                    heap_mb=entry.get("heap_mb", plan["heap_mb"]), limit=LIMIT,
                    sizes=plan["sizes"], case_manifest=CASES, controller=os.environ.get("PERF_CONTROLLER_ID", "local"),
                    generator_sources=hashlib.sha256(b"".join(p.read_bytes() for p in sorted(
                        (ROOT / "tools/edgen/src").rglob("*.rs")))).hexdigest())
    baseline_path = a.baseline_dir / f"{identity['host']}-{a.place}-{a.tier}-{entry['engine']}.json"
    base = json.loads(baseline_path.read_text()) if baseline_path.exists() else {}
    er.result["identity"] = identity
    if base and base.get("identity") != identity:
        er.result["summary"] = [dict(case=c["id"], status="FAIL: baseline identity mismatch") for c in entry["cases"]]
        er.result["failures"].append("baseline identity mismatch; use another baseline directory/host id")
        er.close()
        write_json(directory / "engine.json", er.result)
        return er.result
    try:
        er.start()
        for dataset in dict.fromkeys(c["dataset"] for c in entry["cases"]):
            try:
                try:
                    er.seed(dataset)
                except TimeoutError as e:
                    # A single stalled engine request (seen once: ES 9.1.2 edge seed, 300 s) must not
                    # discard the dataset's 12 cases: log it, drop the partial index and seed once more.
                    er.result.setdefault("seed_retries", []).append(f"{dataset}: {e}")
                    print(f"warning: seed {dataset} stalled ({e}); retrying once", flush=True)
                    er.cleanup_dataset()
                    er.seed(dataset)
                for case in [c for c in entry["cases"] if c["dataset"] == dataset]:
                    if skip_reason(entry["engine"], case, "rs"):
                        er.result["summary"].append(dict(case=case["id"], status="SKIP: " + skip_reason(entry["engine"], case, "rs")))
                        continue
                    variants = ["rs"] + ([] if a.skip_node or skip_reason(entry["engine"], case, "node") else ["node"])
                    samples = {v: [] for v in variants}
                    failed = False
                    node_status = None
                    # A verified untimed warm-up per variant, then rotated paired order per repetition.
                    for rep in range(-1, a.reps):
                        order = [v for v in variants if not (v == "node" and node_status)]
                        random.Random(173 + rep).shuffle(order)
                        for tool in order:
                            try:
                                m = er.iteration(case, tool, rep)
                                if rep >= 0:
                                    samples[tool].append(m)
                            except Exception as e:
                                if tool == "node":
                                    # Node is the comparison, not the product under test: its failure or
                                    # data difference is recorded (like Node SLOWER) and never fails rs.
                                    node_status = ("NODE_DIFF" if isinstance(e, VerifyMismatch) else
                                                   "NODE_TIMEOUT" if isinstance(e, ToolTimeout) else "NODE_FAIL")
                                    er.result.setdefault("node_differences", []).append(
                                        f"{case['id']}/node/rep{rep}: {node_status}: {e}")
                                else:
                                    er.result["failures"].append(f"{case['id']}/{tool}/rep{rep}: {e}")
                                    failed = True
                        if failed:
                            break
                    stats = {v: aggregate(ms) for v, ms in samples.items() if len(ms) == a.reps and not failed}
                    row = row_summary(case["id"], stats, base.get("cells", {}).get(case["id"]), a.threshold,
                                      directory / ("compare-" + case["id"]))
                    row["node_status"] = node_status
                    row["node_skip"] = "--skip-node" if a.skip_node else skip_reason(entry["engine"], case, "node")
                    if row["status"] == "REGRESSION":
                        er.result["failures"].append(case["id"] + ": baseline regression")
                    er.result["summary"].append(row)
            except Exception as e:
                er.result["failures"].append(f"dataset {dataset}: {e}")
            finally:
                er.cleanup_dataset()
    except Exception as e:
        er.result["failures"].append("engine: " + str(e))
    finally:
        # Seed/setup failures must remain visible as table rows, not silently disappear.
        present = {r["case"] for r in er.result["summary"]}
        er.result["summary"] += [dict(case=c["id"], status="FAIL: setup/seed interrupted")
                                 for c in entry["cases"] if c["id"] not in present]
        # Write diagnostics before teardown too, so an interrupted down.sh loses no samples.
        write_json(directory / "engine.json", er.result)
        er.close()
    if a.update_baselines and not er.result["failures"]:
        cells = dict(base.get("cells", {}))
        cells.update({r["case"]: r["rs"] for r in er.result["summary"] if r.get("rs")})
        write_json(baseline_path, dict(identity=identity, cells=cells, recorded=now()))
        er.result["baseline_updated"] = str(baseline_path)
    write_json(directory / "engine.json", er.result)
    return er.result


def now():
    return datetime.datetime.now().astimezone().isoformat(timespec="seconds")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--engines", help="comma-separated stable profiles (see --dry-run)")
    ap.add_argument("--cases", help="comma-separated case IDs (see --dry-run)")
    ap.add_argument("--tier", choices=("L0", "L1"), default="L0")
    ap.add_argument("--place", default=os.environ.get("LOAD_PLACE", "big"))
    ap.add_argument("--reps", type=int, default=5)
    ap.add_argument("--skip-node", action="store_true")
    ap.add_argument("--baseline-dir", type=Path, default=HERE / "baselines")
    ap.add_argument("--update-baselines", action="store_true")
    ap.add_argument("--threshold", type=float, default=10)
    ap.add_argument("--dry-run", action="store_true")
    a = ap.parse_args()
    if a.reps < 1 or not 0 <= a.threshold < 100:
        ap.error("--reps must be positive; --threshold must be in [0,100)")
    try:
        plan = make_plan(a)
        if a.dry_run:
            print(json.dumps(plan, indent=2))
            return 0
        host = os.environ.get("PERF_HOST_ID", platform.node())
        if not re.fullmatch(r"[A-Za-z0-9][A-Za-z0-9_.-]{0,127}", host):
            raise ValueError("PERF_HOST_ID must be a simple alphanumeric/dot/dash/underscore machine name")
        a.baseline_dir = a.baseline_dir.resolve()
        if not a.baseline_dir.is_relative_to(ROOT):
            raise ValueError("--baseline-dir must be inside the mounted repository")
        # One lock covers the ENTIRE sequence, including prerequisite builds and teardown.
        lock_path = Path("/tmp/searchdump-it.lock")
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("w") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            tools = prerequisites(a, plan)
            need_mem = plan["heap_mb"] * 2 + 512 + plan["place_config"]["tool_mem_mb"] + 256
            outdir = ROOT / "results/perf" / (datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + f"-{os.getpid()}")
            outdir.mkdir(mode=0o700, parents=True)
            result = dict(schema=PROTOCOL, started=now(), plan=plan, engines=[],
                          git=capture(["git", "rev-parse", "HEAD"], cwd=ROOT),
                          tool_hashes={k: hashlib.sha256(Path(v).read_bytes()).hexdigest() for k, v in tools.items()},
                          node_runtime=capture(["node", "--version"]) if not a.skip_node else None)
            for entry in plan["engines"]:
                # Guard against the engine's own heap, not only the tier default.
                # Observed container RSS is ~1.1-1.3× heap for these images; keep 2× for small
                # heaps and 1.5× once heap ≥ 2g (room for an intentional per-engine raise).
                _h = entry.get("heap_mb", plan["heap_mb"])
                _mult = 1.5 if _h >= 2048 else 2
                eng_need = int(_h * _mult) + 512 + plan["place_config"]["tool_mem_mb"] + 256
                if R.meminfo_avail_mb() < eng_need or R.disk_free_mb(str(outdir)) < plan["need_disk_mb"]:
                    result["engines"].append(dict(engine=entry["engine"], failures=["RAM/disk resource guard refused engine"],
                                                         summary=[dict(case=c["id"], status="REFUSED: resources") for c in entry["cases"]]))
                    continue
                directory = outdir / entry["engine"]
                directory.mkdir()
                try:
                    result["engines"].append(execute_engine(a, plan, entry, tools, directory))
                except TeardownError as e:
                    diag = directory / "engine.json"
                    record = json.loads(diag.read_text()) if diag.exists() else dict(engine=entry["engine"], summary=[], failures=[])
                    record["failures"].append(str(e))
                    result["engines"].append(record)
                    for pending in plan["engines"][len(result["engines"]):]:
                        result["engines"].append(dict(engine=pending["engine"], failures=["not run: prior teardown failed"],
                                                     summary=[dict(case=c["id"], status="NOT_RUN: teardown") for c in pending["cases"]]))
                    break
                except Exception as e:
                    result["engines"].append(dict(engine=entry["engine"], failures=[str(e)],
                                                 summary=[dict(case=c["id"], status="FAIL: orchestration") for c in entry["cases"]]))
                write_json(outdir / "combined.json", result)
                (outdir / "summary.md").write_text(markdown(result))
            result["finished"] = now()
            result["pass"] = not any(e["failures"] for e in result["engines"])
            write_json(outdir / "combined.json", result)
            (outdir / "summary.md").write_text(markdown(result))
            print(f"PERF-SUMMARY {'PASS' if result['pass'] else 'FAIL'} {outdir / 'summary.md'}")
            return 0 if result["pass"] else 1
    except (ValueError, BlockingIOError) as e:
        print("perf: " + str(e), file=sys.stderr)
        return 2


def interrupted(signum, _frame):
    # Unwinds engine finally/teardown on SIGTERM as well as keyboard interrupt.
    raise KeyboardInterrupt(f"signal {signum}")


if __name__ == "__main__":
    os.umask(0o077)
    signal.signal(signal.SIGTERM, interrupted)
    sys.exit(main())
