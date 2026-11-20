#!/usr/bin/env python3
"""1.3 decision 2 evidence (Robert t227u/t228u): repeated, interleaved small-heap loads that compare the 429 policies
(1.2.0 auto, 1.3 floor default, floor+growback, floor+retryfirst+growback) with grow-back counters.

    growback_reps.py --engine os3 --heap 256m --workload load|multi8 --reps 4 \
        --variant NAME=BINDIR:MODE:EXPERIMENT [--variant ...] [--setting k=v ...] [--port 19415] [--out DIR]

load    one index `pmc` (20,000 x ~22 KB) with `searchdump-multi --direction load` (one child)
multi8  eight indexes `pmc-0..7` (8 x 2,500 x ~22 KB) with `--parallel 8` (the small-heap analogue of Suite A
        multi-restore-p8)
--setting k=v  extra static node setting (official Elasticsearch images only, IT_ES_EXTRA_SETTINGS), e.g.
        indexing_pressure.memory.limit=2mb for a sustained indexing-pressure 429.

Per run: wall, exit, documents counted after the load (and whether they match), size 429s, splits, retryfirst resends,
grows, failed grows with their penalty (from the 1.3 `bulk size summary` line; 1.2.0 has no summary, so 429s and
splits are counted from its warnings), lowest/final cap, parent-breaker trips and the node's heap peak (polled
every 0.5 s from _nodes/stats/jvm).
"""
import argparse, glob, json, os, re, shutil, statistics as st, subprocess, sys, tempfile, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import lowheap as L  # noqa: E402
import run as R  # noqa: E402

DATA = os.path.join(os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"), "lowheap-data")
SUMMARY = re.compile(r"bulk size summary: size 429s (\d+), splits (\d+), retryfirst resends (\d+), grows (\d+), "
                     r"failed grows (\d+) \((\d+) ms, (\d+) KB refused above the pre-grow cap\), lowest cap (\d+) KB, "
                     r"final cap (\d+) KB")


def trips(cl):
    s = cl.req("GET", "/_nodes/stats/breaker", retry_429=False)
    return sum(n["breakers"].get("parent", {}).get("tripped", 0) for n in s["nodes"].values())


class HeapPeak(threading.Thread):
    def __init__(self, cl):
        super().__init__(daemon=True)
        self.cl, self.peak, self.stop = cl, 0, threading.Event()

    def run(self):
        while not self.stop.is_set():
            try:
                s = self.cl.req("GET", "/_nodes/stats/jvm?filter_path=nodes.*.jvm.mem.heap_used_percent", retry_429=False)
                for n in s["nodes"].values():
                    self.peak = max(self.peak, n["jvm"]["mem"]["heap_used_percent"])
            except Exception:
                pass
            self.stop.wait(0.5)


def parse(text):
    r = dict(size429=0, splits=0, retryfirst=0, grows=0, failed_grows=0, fg_ms=0, fg_kb=0, low_cap_kb=0,
             final_cap_kb=0, summaries=0)
    lows, finals = [], []
    for m in SUMMARY.finditer(text):
        v = list(map(int, m.groups()))
        r["summaries"] += 1
        for k, x in zip(("size429", "splits", "retryfirst", "grows", "failed_grows", "fg_ms", "fg_kb"), v[:7]):
            r[k] += x
        if v[7]:
            lows.append(v[7])
        finals.append(v[8])
    if not r["summaries"]:  # 1.2.0: count the warnings
        r["splits"] = text.count("splitting it in half")
        r["retryfirst"] = text.count("[retryfirst]")
        r["size429"] = r["splits"] + r["retryfirst"]
        r["grows"] = text.count("grows back")
        caps = [int(x) for x in re.findall(r"later bulks at <= (\d+) KB", text)]
        lows = caps
        finals = caps[-1:] if caps else []
    r["low_cap_kb"] = min(lows) if lows else 0
    r["final_cap_kb"] = min(finals) if finals else 0
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", default="os3"); ap.add_argument("--heap", default="256m")
    ap.add_argument("--workload", choices=("load", "multi8"), default="load")
    ap.add_argument("--reps", type=int, default=4)
    ap.add_argument("--variant", action="append", required=True)
    ap.add_argument("--setting", action="append", default=[])
    ap.add_argument("--port", type=int, default=19415)
    ap.add_argument("--out", required=True)
    a = ap.parse_args()
    variants = []
    for v in a.variant:
        name, rest = v.split("=", 1)
        bindir, mode, exp = (rest.split(":") + ["", ""])[:3]
        variants.append(dict(name=name, bindir=bindir, mode=mode, exp=exp))
    os.makedirs(a.out, exist_ok=True)
    work = tempfile.mkdtemp(prefix="l3gb-", dir=os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"))
    eng = L.Engine(a.engine, a.heap, work, a.port)
    if a.setting:
        eng.env["IT_ES_EXTRA_SETTINGS"] = " ".join(a.setting)
    if a.workload == "load":
        idx = {"pmc": os.path.join(DATA, "pmc.json")}
        match, parallel = "^pmc$", []
    else:
        idx = {f"pmc-{i}": os.path.join(DATA, f"pmc-8-{i}.json") for i in range(8)}
        match, parallel = "^pmc-[0-7]$", ["--parallel", "8"]
    rows = []
    try:
        version = eng.up()
        cl = eng.cl
        b0 = variants[0]["bindir"]
        expected = 0
        for name, src in idx.items():
            cl.req("PUT", f"/{name}", L.MAPPING)
            rc = subprocess.run([f"{b0}/searchdump", f"--input={src}", f"--output={eng.url}/{name}", "--type=data",
                                 "--limit=200", "--serverLimits=off"],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
            assert rc == 0, f"seed {name} failed"
            cl.req("POST", f"/{name}/_refresh")
            expected += cl.req("GET", f"/{name}/_count")["count"]
        dump = os.path.join(work, "dump")
        rc = subprocess.run([f"{b0}/searchdump-multi", "--direction", "dump", "--input", eng.url, "--output", dump,
                             "--match", match, "--serverLimits=off"],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode
        assert rc == 0, "dump failed"
        for rep in range(a.reps):
            order = variants[rep % len(variants):] + variants[:rep % len(variants)]
            for v in order:
                for name in idx:
                    cl.req("DELETE", f"/{name}", ok404=True)
                logdir = os.path.join(work, f"logs-{v['name']}-{rep}")
                env = dict(os.environ, SEARCHDUMP_LIMITS_EXPERIMENT=v["exp"])
                argv = [f"{v['bindir']}/searchdump-multi", "--direction", "load", "--input", dump, "--output", eng.url,
                        "--match", match, "--logDir", logdir, *parallel] + \
                       ([] if v["mode"] in ("", "none") else [f"--serverLimits={v['mode']}"])
                t_before = trips(cl)
                hp = HeapPeak(cl); hp.start()
                t0 = time.time()
                p = subprocess.run(argv, capture_output=True, text=True, env=env)
                wall = round(time.time() - t0, 2)
                hp.stop.set(); hp.join()
                text = p.stdout + p.stderr
                for f in glob.glob(os.path.join(logdir, "*.log")):
                    text += open(f, errors="replace").read()
                alive = eng.alive()
                cnt = 0
                for name in idx:
                    try:
                        cl.req("POST", f"/{name}/_refresh")
                        cnt += cl.req("GET", f"/{name}/_count")["count"]
                    except Exception:
                        pass
                r = dict(rep=rep, variant=v["name"], exit=p.returncode, wall_s=wall, count=cnt, expected=expected,
                         count_ok=cnt == expected, alive=alive, parent_trips=trips(cl) - t_before, heap_peak_pct=hp.peak,
                         **parse(text))
                rows.append(r)
                print(json.dumps(r), flush=True)
                if not alive:
                    eng.restart()
        res = dict(engine=a.engine, version=version, heap=a.heap, workload=a.workload, settings=a.setting,
                   variants=variants, rows=rows)
        json.dump(res, open(os.path.join(a.out, "reps.json"), "w"), indent=1)
        print(f"\n{a.engine} {version} {a.heap} {a.workload} {' '.join(a.setting)}")
        print("| variant | n | median s | min | max | count ok | size 429s/run | splits/run | grows/run | failed grows/run "
              "| failed-grow ms/run | lowest cap KB | heap peak % |\n|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")
        for v in variants:
            rs = [r for r in rows if r["variant"] == v["name"]]
            ws = [r["wall_s"] for r in rs]
            m = lambda k: st.mean(r[k] for r in rs)  # noqa: E731
            print(f"| {v['name']} | {len(rs)} | {st.median(ws):.1f} | {min(ws):.1f} | {max(ws):.1f} | "
                  f"{sum(r['count_ok'] and r['exit'] == 0 for r in rs)}/{len(rs)} | {m('size429'):.1f} | {m('splits'):.1f} | "
                  f"{m('grows'):.1f} | {m('failed_grows'):.1f} | {m('fg_ms'):.0f} | "
                  f"{min([r['low_cap_kb'] for r in rs if r['low_cap_kb']] or [0])} | {max(r['heap_peak_pct'] for r in rs)} |")
    finally:
        eng.down()
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
