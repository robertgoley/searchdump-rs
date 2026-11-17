#!/usr/bin/env python3
"""1.3 stall characterisation: repeat searchdump-multi restores against one plain engine through
scripts/perf/trace_proxy.py and record, per run, the wall time, the slowest requests and engine
counters (GC, refresh, flush, merges, pending tasks) before and after, so a multi-second stall can be
attributed to a request kind and to engine work.

    stall_loop.py ENGINE_URL PROXY_URL TRACE_TSV OUTDIR BINDIR --refs DIR --reps N --parallel 1,8
Plain HTTP only; the engine must already hold the perf-src-NN indexes (scripts/perf/ab_os3g.py seeds them).
"""
import argparse, json, os, subprocess, time, urllib.request

def get(url):
    try:
        with urllib.request.urlopen(url, timeout=60) as f:
            return json.loads(f.read())
    except Exception as e:
        return {"error": str(e)}

def delete(url, name):
    r = urllib.request.Request(f"{url}/{name}", method="DELETE")
    try:
        urllib.request.urlopen(r, timeout=120).read()
    except Exception:
        pass
    for _ in range(600):
        try:
            urllib.request.urlopen(urllib.request.Request(f"{url}/{name}", method="HEAD"), timeout=30)
        except urllib.error.HTTPError as e:
            if e.code == 404:
                return
        time.sleep(0.05)

def counters(url):
    s = get(f"{url}/_nodes/stats/jvm,indices,thread_pool")
    out = {}
    for n in (s.get("nodes") or {}).values():
        gc = n["jvm"]["gc"]["collectors"]
        out["gc_young_ms"] = gc.get("young", {}).get("collection_time_in_millis")
        out["gc_old_ms"] = gc.get("old", {}).get("collection_time_in_millis")
        out["gc_old_n"] = gc.get("old", {}).get("collection_count")
        out["heap_pct"] = n["jvm"]["mem"]["heap_used_percent"]
        ix = n["indices"]
        out["refresh_ms"] = ix["refresh"]["total_time_in_millis"]
        out["flush_ms"] = ix["flush"]["total_time_in_millis"]
        out["merge_ms"] = ix["merges"]["total_time_in_millis"]
        out["write_rejected"] = n["thread_pool"].get("write", {}).get("rejected")
    out["pending_tasks"] = len(get(f"{url}/_cluster/pending_tasks").get("tasks", []))
    return out

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("url"); ap.add_argument("proxy"); ap.add_argument("trace"); ap.add_argument("out"); ap.add_argument("bindir")
    ap.add_argument("--refs", required=True); ap.add_argument("--reps", type=int, default=10)
    ap.add_argument("--parallel", default="1,8"); ap.add_argument("--n", type=int, default=16)
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    res = []
    for rep in range(a.reps):
        for p in a.parallel.split(","):
            for i in range(a.n):
                delete(a.url, f"perf-dst-perf-src-{i:02d}")
            before = counters(a.url)
            mark = os.path.getsize(a.trace) if os.path.exists(a.trace) else 0
            t0 = time.time()
            cmd = [f"{a.bindir}/searchdump-multi", "--direction=load", f"--input={a.refs}", f"--output={a.proxy}",
                   "--includeType=data", "--interval=0", f"--parallel={p}", "--prefix=perf-dst-", "--compression=none",
                   "--noDefaultDataFlags", f"--logDir={a.out}/logs-{p}-{rep}", "--", "--limit=2000", "--concurrency=1"]
            rc = subprocess.run(cmd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
            wall = time.time() - t0
            after = counters(a.url)
            reqs = []
            with open(a.trace) as f:
                f.seek(mark)
                for line in f:
                    t = line.rstrip("\n").split("\t")
                    if len(t) >= 6:
                        reqs.append((float(t[5]), t[1], t[2], t[3][:60], t[4], float(t[0]) - t0))
            reqs.sort(reverse=True)
            by = {}
            for ms, m, path, q, st, _ in reqs:
                k = m + " " + ("/IDX/" + path.split("/", 2)[2] if path.count("/") >= 2 and path.startswith("/perf") else path)
                by.setdefault(k, [0, 0.0]); by[k][0] += 1; by[k][1] += ms
            r = dict(rep=rep, parallel=int(p), rc=rc, wall_s=round(wall, 3), started=time.strftime("%H:%M:%S", time.localtime(t0)),
                     slowest=[dict(ms=x[0], req=f"{x[1]} {x[2]} {x[3]}", status=x[4], at_s=round(x[5], 2)) for x in reqs[:5]],
                     by_kind={k: dict(n=v[0], total_ms=round(v[1])) for k, v in by.items()},
                     delta={k: (after[k] - before[k]) if isinstance(after.get(k), (int, float)) and isinstance(before.get(k), (int, float)) else after.get(k) for k in after})
            res.append(r)
            print(json.dumps({k: r[k] for k in ("rep", "parallel", "rc", "wall_s", "started")}), "slowest", r["slowest"][0] if r["slowest"] else None, "delta", r["delta"], flush=True)
            json.dump(res, open(os.path.join(a.out, "stalls.json"), "w"), indent=1)

if __name__ == "__main__":
    main()
