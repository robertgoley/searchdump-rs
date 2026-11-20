#!/usr/bin/env python3
"""1.3-G: interleaved A/B of two searchdump release dirs on ONE running engine.

Runs the relcmp Suite A metadata and multi p8 cases with identical flags, alternating the
binaries rep by rep (A B A B ...) so engine state over time cannot favour one release.
Usage: ab_os3g.py URL OUTDIR NAME=BINDIR NAME=BINDIR [--reps N] [--docs N] [--cases a,b]
Plain HTTP engines only (no credentials handled here).
"""
import argparse, json, os, shutil, subprocess, sys, time, urllib.request

def req(url, method="GET", body=None, ctype="application/json"):
    r = urllib.request.Request(url, method=method, data=body, headers={"Content-Type": ctype})
    try:
        with urllib.request.urlopen(r, timeout=120) as f:
            return f.status, f.read()
    except urllib.error.HTTPError as e:
        return e.code, e.read()

def seed(url, n_idx, docs):
    for i in range(n_idx):
        idx = f"perf-src-{i:02d}"
        if req(f"{url}/{idx}", "HEAD")[0] == 200:
            continue
        req(f"{url}/{idx}", "PUT", json.dumps({"settings": {"number_of_shards": 1, "number_of_replicas": 0}}).encode())
        for start in range(0, docs, 2000):
            lines = []
            for d in range(start, min(docs, start + 2000)):
                lines.append(json.dumps({"index": {"_index": idx, "_id": str(d)}}))
                lines.append(json.dumps({"ts": 1700000000 + d, "host": f"h{d % 50}", "level": "INFO",
                                         "msg": ("log line %d " % d) * 40, "n": d}))
            s, b = req(f"{url}/_bulk", "POST", ("\n".join(lines) + "\n").encode(), "application/x-ndjson")
            assert s == 200 and b'"errors":false' in b, b[:300]
    # perf-src for single-index metadata cases
    if req(f"{url}/perf-src", "HEAD")[0] != 200:
        req(f"{url}/perf-src", "PUT", json.dumps({"settings": {"number_of_shards": 1, "number_of_replicas": 0},
            "mappings": {"properties": {"ts": {"type": "long"}, "host": {"type": "keyword"},
                                        "level": {"type": "keyword"}, "msg": {"type": "text"}, "n": {"type": "long"}}}}).encode())
    req(f"{url}/_refresh", "POST")

def delete(url, name):
    s, b = req(f"{url}/{name}", "DELETE")
    # wait until really gone (the 1.3-R1 harness lesson)
    for _ in range(200):
        if req(f"{url}/{name}", "HEAD")[0] == 404:
            return s
        time.sleep(0.05)
    return s

def run(cmd, log):
    t = time.monotonic()
    with open(log, "w") as f:
        p = subprocess.run(cmd, stdout=f, stderr=subprocess.STDOUT)
    return time.monotonic() - t, p.returncode

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("url"); ap.add_argument("out"); ap.add_argument("bins", nargs=2)
    ap.add_argument("--reps", type=int, default=5); ap.add_argument("--docs", type=int, default=6250)
    ap.add_argument("--cases", default="mapping-dump,mapping-restore,settings-dump,settings-restore,multi-dump-p8,multi-restore-p8")
    a = ap.parse_args()
    bins = [b.split("=", 1) for b in a.bins]
    out = os.path.abspath(a.out); os.makedirs(out, exist_ok=True)
    seed(a.url, 16, a.docs)
    work = os.path.join(out, "work"); os.makedirs(work, exist_ok=True)
    ref = bins[0][1]
    # references made once with the first binary
    for t in ("mapping", "settings"):
        subprocess.run([f"{ref}/searchdump", f"--input={a.url}/perf-src", f"--output={work}/ref-{t}.json", f"--type={t}"],
                       check=True, capture_output=True)
    refs = os.path.join(work, "refs"); shutil.rmtree(refs, ignore_errors=True)
    subprocess.run([f"{ref}/searchdump-multi", "--direction=dump", f"--input={a.url}", f"--output={refs}", "--includeType=data",
                    "--parallel=8", "--nodeCompat", "--match=^perf-src-", "--matchType=index", "--compression=none",
                    "--noDefaultDataFlags", f"--logDir={work}/refs-logs", "--", "--limit=2000", "--concurrency=1"], check=True, capture_output=True)
    rows = []
    cases = a.cases.split(",")
    for rep in range(-1, a.reps):
        for case in cases:
            order = bins if rep % 2 == 0 else bins[::-1]
            for name, bd in order:
                tag = f"{case}-{name}-{rep}"
                logdir = os.path.join(out, "logs"); os.makedirs(logdir, exist_ok=True)
                if case.endswith("-dump") and not case.startswith("multi"):
                    try: os.remove(f"{work}/o.json")
                    except FileNotFoundError: pass
                if case == "mapping-dump":
                    cmd = [f"{bd}/searchdump", f"--input={a.url}/perf-src", f"--output={work}/o.json", "--limit=2000", "--concurrency=1", "--type=mapping"]
                elif case == "settings-dump":
                    cmd = [f"{bd}/searchdump", f"--input={a.url}/perf-src", f"--output={work}/o.json", "--limit=2000", "--concurrency=1", "--type=settings"]
                elif case in ("mapping-restore", "settings-restore"):
                    t = case.split("-")[0]
                    delete(a.url, "perf-dst")
                    if t == "mapping":  # suite creates the target with settings first
                        req(f"{a.url}/perf-dst", "PUT", json.dumps({"settings": {"number_of_shards": 1, "number_of_replicas": 0}}).encode())
                    cmd = [f"{bd}/searchdump", f"--input={work}/ref-{t}.json", f"--output={a.url}/perf-dst", "--limit=2000", "--concurrency=1", f"--type={t}"]
                elif case.startswith("multi-dump-p"):
                    p = case.rsplit("p", 1)[1]
                    d = os.path.join(work, "mout"); shutil.rmtree(d, ignore_errors=True)
                    cmd = [f"{bd}/searchdump-multi", "--direction=dump", f"--input={a.url}", f"--output={d}", "--includeType=data", "--interval=0",
                           f"--parallel={p}", "--nodeCompat", "--match=^perf-src-", "--matchType=index", "--compression=none", "--noDefaultDataFlags",
                           f"--logDir={logdir}/{tag}", "--", "--limit=2000", "--concurrency=1"]
                elif case.startswith("multi-restore-p"):
                    p = case.rsplit("p", 1)[1]
                    for i in range(16):
                        delete(a.url, f"perf-dst-perf-src-{i:02d}")
                    cmd = [f"{bd}/searchdump-multi", "--direction=load", f"--input={refs}", f"--output={a.url}", "--includeType=data", "--interval=0",
                           f"--parallel={p}", "--prefix=perf-dst-", "--compression=none", "--noDefaultDataFlags", f"--logDir={logdir}/{tag}",
                           "--", "--limit=2000", "--concurrency=1"]
                wall, rc = run(cmd, os.path.join(logdir, tag + ".log"))
                rows.append(dict(case=case, bin=name, rep=rep, wall_s=round(wall, 4), rc=rc, t=time.strftime("%H:%M:%S")))
                print(json.dumps(rows[-1]), flush=True)
    json.dump(rows, open(os.path.join(out, "ab.json"), "w"), indent=1)
    import statistics as st
    print("\ncase                 " + "  ".join(f"{n:>9s}" for n, _ in bins) + "  ratio")
    for case in cases:
        med = [st.median([r["wall_s"] for r in rows if r["case"] == case and r["bin"] == n and r["rep"] >= 0]) for n, _ in bins]
        print(f"{case:20s} " + "  ".join(f"{m:9.3f}" for m in med) + f"  {med[1]/med[0]:5.2f}")

if __name__ == "__main__":
    main()
