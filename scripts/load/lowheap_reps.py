#!/usr/bin/env python3
"""1.3-D+: repeated, interleaved small-heap LOAD runs on ONE engine, to compare 429 strategies with more
than one sample per cell (lowheap.py runs each cell once, and breaker trips depend on GC timing).

    lowheap_reps.py --engine os3 --heap 256m --dataset pmc --reps 5 \
        --variant NAME=BINDIR:MODE:EXPERIMENT [--variant ...] [--port 19415] [--out DIR]

MODE is a --serverLimits value or `none`; EXPERIMENT is a SEARCHDUMP_LIMITS_EXPERIMENT value or empty.
The dump directory is made once with the first variant's binary. Each rep deletes the index and loads it
with every variant, rotating the order per rep. Records wall, exit, split / halve / retryfirst / parent
retry events (from the kept child logs) and the node's parent-breaker trip count delta.
"""
import argparse, glob, json, os, re, shutil, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import lowheap as L  # noqa: E402
import run as R  # noqa: E402


def trips(cl):
    s = cl.req("GET", "/_nodes/stats/breaker")
    return sum(n["breakers"].get("parent", {}).get("tripped", 0) for n in s["nodes"].values())


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", default="os3"); ap.add_argument("--heap", default="256m")
    ap.add_argument("--dataset", default="pmc"); ap.add_argument("--reps", type=int, default=5)
    ap.add_argument("--variant", action="append", required=True); ap.add_argument("--port", type=int, default=19415)
    ap.add_argument("--out", default=os.path.join(R.ROOT, "results", "load", "lowheap-reps-" + time.strftime("%Y%m%dT%H%M%S")))
    a = ap.parse_args()
    variants = []
    for v in a.variant:
        name, rest = v.split("=", 1)
        bindir, mode, exp = (rest.split(":") + ["", ""])[:3]
        variants.append(dict(name=name, bindir=bindir, mode=mode, exp=exp))
    os.makedirs(a.out, exist_ok=True)
    work = tempfile.mkdtemp(prefix="l3reps-", dir=os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"))
    src = os.path.join(os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"), "lowheap-data", f"{a.dataset}.json")
    eng = L.Engine(a.engine, a.heap, work, a.port)
    rows = []
    try:
        version = eng.up()
        cl = eng.cl
        d = a.dataset
        cl.req("PUT", f"/{d}", L.MAPPING)
        b0 = variants[0]["bindir"]
        lim = "200" if d == "pmc" else "20"
        rc = subprocess.run([f"{b0}/searchdump", f"--input={src}", f"--output={eng.url}/{d}", "--type=data", f"--limit={lim}"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
        assert rc == 0, "seed failed"
        cl.req("POST", f"/{d}/_refresh")
        dump = os.path.join(work, "dump")
        rc = subprocess.run([f"{b0}/searchdump-multi", "--direction", "dump", "--input", eng.url, "--output", dump,
                             "--match", f"^{d}$"] + ([] if variants[0]["mode"] in ("", "none") else ["--serverLimits=off"]),
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
        assert rc == 0, "dump failed"
        for rep in range(a.reps):
            order = variants[rep % len(variants):] + variants[:rep % len(variants)]
            for v in order:
                cl.req("DELETE", f"/{d}", ok404=True)
                logdir = os.path.join(work, f"logs-{v['name']}-{rep}")
                env = dict(os.environ, SEARCHDUMP_LIMITS_EXPERIMENT=v["exp"])
                argv = [f"{v['bindir']}/searchdump-multi", "--direction", "load", "--input", dump, "--output", eng.url,
                        "--match", f"^{d}$", "--logDir", logdir] + ([] if v["mode"] in ("", "none") else [f"--serverLimits={v['mode']}"])
                t_before = trips(cl)
                t0 = time.time()
                p = subprocess.run(argv, capture_output=True, text=True, env=env)
                wall = round(time.time() - t0, 2)
                text = p.stdout + p.stderr
                for f in glob.glob(os.path.join(logdir, "*.log")):
                    text += open(f, errors="replace").read()
                alive = eng.alive()
                cl.req("POST", f"/{d}/_refresh")
                cnt = cl.req("GET", f"/{d}/_count")["count"]
                r = dict(rep=rep, variant=v["name"], exit=p.returncode, wall_s=wall, count=cnt, alive=alive,
                         splits=text.count("splitting it in half"), halvings=text.count("halving the page"),
                         retryfirst=text.count("[retryfirst]"), parent_retries=text.count("retrying in") ,
                         grows=text.count("grows back"), parent_trips=trips(cl) - t_before,
                         min_cap_kb=min([int(x) for x in re.findall(r"later bulks at <= (\d+) KB", text)] or [0]))
                rows.append(r)
                print(json.dumps(r), flush=True)
                if not alive:
                    eng.restart()
        res = dict(engine=a.engine, version=version, heap=a.heap, dataset=a.dataset, variants=variants, rows=rows)
        json.dump(res, open(os.path.join(a.out, "reps.json"), "w"), indent=1)
        import statistics as st
        print(f"\n| variant | n | median wall s | min | max | exit!=0 | splits/run | parent trips/run |\n|---|---:|---:|---:|---:|---:|---:|---:|")
        for v in variants:
            rs = [r for r in rows if r["variant"] == v["name"]]
            ws = [r["wall_s"] for r in rs]
            print(f"| {v['name']} | {len(rs)} | {st.median(ws):.1f} | {min(ws):.1f} | {max(ws):.1f} | {sum(r['exit'] != 0 for r in rs)} | "
                  f"{st.mean(r['splits'] for r in rs):.1f} | {st.mean(r['parent_trips'] for r in rs):.1f} |")
    finally:
        eng.down()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
