#!/usr/bin/env python3
"""Metrics wrapper (TST-INF-006 / TST-LOAD-003). Stdlib only.

  scripts/load/measure.sh [--run-id ID] [--out FILE] [--cluster URL] [--docs N]
        [--input PATH]... [--output PATH]... [--uncompressed-bytes N] [--label k=v]... -- CMD...

Runs CMD, samples /proc/<pid> every 100 ms on a side thread (RSS, FDs,
threads, I/O), blocks in wait4 so wall time has sub-millisecond resolution
(result field "timer": "blocking-wait4"; older results lack it), takes
rusage at exit, parses the tool's stderr (retries by kind, Total/Failed
Writes), samples the cluster's _nodes/stats before and after, and writes
results/<run-id>.json (schema: scripts/load/metrics.schema.json). The
command's stdout/stderr are passed through (stderr also teed to <out>.stderr).

Cgroup accounting (systemd-run --user --scope / podman --cgroups) is not
available on the box (no root, rootless cgroupfs); RSS comes from rusage
and /proc sampling instead.
"""
import argparse, base64, datetime, json, os, re, ssl, subprocess, sys, threading, time, urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

RETRY_KINDS = [
    ("bulk_429_503", re.compile(r"rejected with 429/503")),
    ("http_429", re.compile(r"\b429\b")),
    ("http_5xx", re.compile(r"\b50[0-9]\b")),
    ("timeout", re.compile(r"timed? ?out", re.I)),
    ("connection", re.compile(r"connect|reset|broken pipe|EOF", re.I)),
]


def classify(line):
    for k, r in RETRY_KINDS:
        if r.search(line):
            return k
    return "other"


def cluster_get(url, path):
    u = urllib.request.urlparse(url)
    base = f"{u.scheme}://{u.hostname}:{u.port or (443 if u.scheme == 'https' else 80)}"
    req = urllib.request.Request(base + path)
    user = u.username or os.environ.get("EDV_USER")
    pw = u.password or os.environ.get("EDV_PASSWORD")
    if user:
        req.add_header("Authorization", "Basic " + base64.b64encode(f"{user}:{pw or ''}".encode()).decode())
    ctx = ssl._create_unverified_context() if u.scheme == "https" else None
    with urllib.request.urlopen(req, timeout=30, context=ctx) as r:
        return json.load(r)


def cluster_sample(url):
    try:
        s = cluster_get(url, "/_nodes/stats/indices,thread_pool")
    except Exception as e:  # noqa: BLE001
        return {"error": str(e)}
    out = {"index_total": 0, "query_total": 0, "scroll_total": 0, "scroll_current": 0,
           "point_in_time_current": None, "write_rejected": 0, "search_rejected": 0}
    for n in s.get("nodes", {}).values():
        ix = n.get("indices", {})
        out["index_total"] += ix.get("indexing", {}).get("index_total", 0)
        se = ix.get("search", {})
        out["query_total"] += se.get("query_total", 0)
        out["scroll_total"] += se.get("scroll_total", 0)
        out["scroll_current"] += se.get("scroll_current", 0)
        if "point_in_time_current" in se:
            out["point_in_time_current"] = (out["point_in_time_current"] or 0) + se["point_in_time_current"]
        tp = n.get("thread_pool", {})
        for k in ("write", "bulk"):
            out["write_rejected"] += tp.get(k, {}).get("rejected", 0)
        out["search_rejected"] += tp.get("search", {}).get("rejected", 0)
    return out


def proc_sample(pid, st):
    """Sample the process and all its descendants."""
    pids = [pid]
    try:
        kids = open(f"/proc/{pid}/task/{pid}/children").read().split()
        pids += [int(k) for k in kids]
    except OSError:
        pass
    rss = fds = thr = 0
    for p in pids:
        try:
            for line in open(f"/proc/{p}/status"):
                if line.startswith("VmRSS:"):
                    rss += int(line.split()[1])
                elif line.startswith("Threads:"):
                    thr += int(line.split()[1])
            fds += len(os.listdir(f"/proc/{p}/fd"))
        except OSError:
            pass
    try:
        io = dict(l.split(": ") for l in open(f"/proc/{pid}/io").read().splitlines())
        st["read_bytes"] = int(io["read_bytes"])
        st["write_bytes"] = int(io["write_bytes"])
    except (OSError, KeyError, ValueError):
        pass
    st["rss"] = max(st["rss"], rss)
    st["fds"] = max(st["fds"], fds)
    st["thr"] = max(st["thr"], thr)


SAMPLE_INTERVAL_S = 0.1
TIMER = "blocking-wait4"


def sample_loop(pid, st, done, interval=SAMPLE_INTERVAL_S):
    """Sample /proc/<pid> until `done` is set; tolerates the process vanishing."""
    while not done.is_set():
        try:
            proc_sample(pid, st)
        except Exception:  # noqa: BLE001 - a racing exit must never kill the sampler
            pass
        done.wait(interval)


def wait_child(pid):
    """Block until `pid` exits; returns (status, rusage). Retries on EINTR (PEP 475 covers
    most cases, but a ChildProcessError must not be swallowed)."""
    while True:
        try:
            wpid, status, ru = os.wait4(pid, 0)
        except InterruptedError:
            continue
        if wpid == pid:
            return status, ru


def sizes(paths):
    tot = 0
    for p in paths:
        if os.path.isdir(p):
            for dp, _, fs in os.walk(p):
                tot += sum(os.path.getsize(os.path.join(dp, f)) for f in fs)
        elif os.path.exists(p):
            tot += os.path.getsize(p)
    return tot


SECRET_FLAGS = re.compile(r"^--(password|pass|input-pass|output-pass|[a-z-]*key-?pass[a-z-]*)$", re.I)


def redact(cmd):
    """Mask secret flag values and URL userinfo before the command is recorded."""
    out, hide = [], False
    for arg in cmd:
        if hide:
            out.append("***")
            hide = False
            continue
        if SECRET_FLAGS.match(arg):
            hide = True
        elif "=" in arg and SECRET_FLAGS.match(arg.split("=", 1)[0]):
            arg = arg.split("=", 1)[0] + "=***"
        arg = re.sub(r"(https?://)[^/@\s]+@", r"\1***@", arg)
        out.append(arg)
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--run-id", default=None)
    ap.add_argument("--out", default=None)
    ap.add_argument("--cluster", default=None)
    ap.add_argument("--docs", type=int, default=None)
    ap.add_argument("--input", action="append", default=[])
    ap.add_argument("--output", action="append", default=[])
    ap.add_argument("--uncompressed-bytes", type=int, default=None)
    ap.add_argument("--label", action="append", default=[])
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    cmd = a.cmd[1:] if a.cmd and a.cmd[0] == "--" else a.cmd
    if not cmd:
        ap.error("missing command after --")
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + f"-{os.getpid()}"
    out = a.out or os.path.join(ROOT, "results", f"{run_id}.json")
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    before = cluster_sample(a.cluster) if a.cluster else None
    started = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
    t0 = time.monotonic()
    p = subprocess.Popen(cmd, stderr=subprocess.PIPE)
    st = {"rss": 0, "fds": 0, "thr": 0, "read_bytes": None, "write_bytes": None}
    retries, writes = {}, {"total": None, "failed": None}
    errlog = open(out + ".stderr", "wb")

    def pump():
        for raw in p.stderr:
            sys.stderr.buffer.write(raw)
            sys.stderr.buffer.flush()
            errlog.write(raw)
            line = raw.decode("utf-8", "replace")
            if "retrying" in line:
                k = classify(line)
                retries[k] = retries.get(k, 0) + 1
            m = re.search(r"Total Writes: (\d+)", line)
            if m:
                writes["total"] = int(m.group(1))
            m = re.search(r"Failed Writes: (\d+)", line)
            if m:
                writes["failed"] = int(m.group(1))

    th = threading.Thread(target=pump, daemon=True)
    th.start()
    # 1.1-5: the main thread blocks in wait4 so wall time is taken the moment the
    # child exits (the old WNOHANG + sleep(0.1) poll rounded every wall time up
    # to the next ~0.1 s tick). /proc sampling moves to a daemon thread.
    done = threading.Event()
    sampler = threading.Thread(target=sample_loop, args=(p.pid, st, done), daemon=True)
    sampler.start()
    status, ru = wait_child(p.pid)
    wall = time.monotonic() - t0
    done.set()
    sampler.join(timeout=1)
    th.join(timeout=5)
    errlog.close()
    code = os.waitstatus_to_exitcode(status)
    p.returncode = code
    after = cluster_sample(a.cluster) if a.cluster else None
    cluster = None
    if a.cluster:
        cluster = {"before": before, "after": after}
        if before and after and "error" not in before and "error" not in after:
            dt = max(wall, 1e-9)
            cluster["indexing_rate"] = (after["index_total"] - before["index_total"]) / dt
            cluster["search_query_rate"] = (after["query_total"] - before["query_total"]) / dt
            cluster["write_rejected"] = after["write_rejected"] - before["write_rejected"]
            cluster["search_rejected"] = after["search_rejected"] - before["search_rejected"]
            cluster["open_scroll_contexts_after"] = after["scroll_current"]
            cluster["open_pit_contexts_after"] = after["point_in_time_current"]
    docs = a.docs if a.docs is not None else writes["total"]
    inb = sizes(a.input) if a.input else None
    outb = sizes(a.output) if a.output else None
    r = {
        "schema": "edrs-metrics/v1",
        "run_id": run_id,
        "started": started,
        "command": redact(cmd),
        "exit_code": code,
        "wall_s": round(wall, 3),
        "timer": TIMER,
        "user_s": round(ru.ru_utime, 3),
        "sys_s": round(ru.ru_stime, 3),
        "cpu_s": round(ru.ru_utime + ru.ru_stime, 3),
        "max_rss_kb": int(ru.ru_maxrss),
        "rss_peak_kb_sampled": st["rss"],
        "fds_peak": st["fds"],
        "threads_peak": st["thr"],
        "vol_ctx_switches": int(ru.ru_nvcsw),
        "invol_ctx_switches": int(ru.ru_nivcsw),
        # rusage block counts include waited-for descendants (512-byte units)
        "read_bytes": int(ru.ru_inblock) * 512,
        "write_bytes": int(ru.ru_oublock) * 512,
        "docs": docs,
        "docs_per_s": round(docs / wall, 1) if docs is not None and wall > 0 else None,
        "input_bytes": inb,
        "output_bytes": outb,
        "mb_per_s_in": round(inb / 1e6 / wall, 3) if inb else None,
        "mb_per_s_out": round(outb / 1e6 / wall, 3) if outb else None,
        "compression_ratio": round(a.uncompressed_bytes / outb, 3) if a.uncompressed_bytes and outb else None,
        "total_writes": writes["total"],
        "failed_writes": writes["failed"],
        "retries": retries,
        "cluster": cluster,
        "host": {"nproc": os.cpu_count(), "kernel": os.uname().release},
        "labels": dict(kv.split("=", 1) for kv in a.label),
    }
    with open(out, "w") as f:
        json.dump(r, f, indent=1)
        f.write("\n")
    errs = validate(r)
    if errs:
        print("measure: schema errors: " + "; ".join(errs), file=sys.stderr)
        return 3
    print(f"measure: {out} wall={r['wall_s']}s cpu={r['cpu_s']}s maxrss={r['max_rss_kb']}KB docs/s={r['docs_per_s']} exit={code}", file=sys.stderr)
    return code


TYPES = {"string": str, "integer": int, "number": (int, float), "object": dict, "array": list, "null": type(None)}


def validate(r, schema_path=os.path.join(os.path.dirname(os.path.abspath(__file__)), "metrics.schema.json")):
    """Minimal JSON-schema check (required + type + const); no third-party module."""
    s = json.load(open(schema_path))
    errs = [f"missing {k}" for k in s["required"] if k not in r]
    for k, spec in s["properties"].items():
        if k not in r:
            continue
        if "const" in spec and r[k] != spec["const"]:
            errs.append(f"{k} != {spec['const']}")
        t = spec.get("type")
        if t:
            ts = t if isinstance(t, list) else [t]
            ok = any(isinstance(r[k], TYPES[x]) and not (x in ("integer", "number") and isinstance(r[k], bool)) for x in ts)
            if not ok:
                errs.append(f"{k}: expected {t}")
    return errs


if __name__ == "__main__":
    if len(sys.argv) > 2 and sys.argv[1] == "--validate":
        bad = 0
        for f in sys.argv[2:]:
            e = validate(json.load(open(f)))
            print(f"{f}: {'OK' if not e else '; '.join(e)}")
            bad |= bool(e)
        sys.exit(bad)
    sys.exit(main())
