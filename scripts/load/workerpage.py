#!/usr/bin/env python3
"""1.2-1b (L2-002): worker / page trade-off matrix on low-heap engines.

    scripts/load/workerpage.py [--engines es710,es819] [--heaps 256m,384m] [--datasets pmc,fat]
                               [--configs 2x5000,4x2500,8x1000[,auto[,off]]] [--indices 8]
                               [--port 19414] [--run-id ID] [--keep]

The lowheap.py datasets (pmc: 20,000 x ~22 KB articles; fat: 3,000 x ~80 KB incompressible) are
spread over N indices (<dataset>-0 .. -N-1, round robin), so `searchdump-multi --parallel P` really
runs P indices at once. For every engine x heap x dataset x config `PxL` it runs

  dump   searchdump-multi --parallel P --match ^<dataset>- -- --limit=L
  load   searchdump-multi --direction load --parallel P (same dump) -- --limit=L
         (bulk bytes stay at the default --maxBulkBytes of the build under test)

and records the wall time, the exit code, whether the node survived (a dead node is restarted and
the dataset re-seeded), HTTP 429 / circuit_breaking / retry counts from the logs, and whether the
dump's (count, digest) equals the source. Config `auto` / `off` run the 1.2 `--serverLimits` modes
with no page or worker flags (only for 1.2 builds). Not a pass/fail gate: a measurement that feeds the
sizing policy (ROADMAP-1.2 section 4.1) and the page `expansion` constant.

LOAD_BIN_DIR selects the binaries (default target/release). Results:
results/load/<run-id>/workerpage.json and a table on stdout.
"""
import argparse, datetime, glob, json, os, shutil, subprocess, sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run as R  # noqa: E402
import lowheap as L  # noqa: E402

ROOT = R.ROOT


def split_dataset(src, n, out_dir, name):
    """Round-robin `src` into n files <name>-i.json (cached)."""
    paths = [os.path.join(out_dir, f"{name}-{n}-{i}.json") for i in range(n)]
    if all(os.path.exists(p) for p in paths):
        return paths
    fos = [open(p + ".tmp", "w") for p in paths]
    with open(src) as fh:
        for k, line in enumerate(fh):
            if not line.strip():
                continue
            d = json.loads(line)
            d["_index"] = f"{name}-{k % n}"
            fos[k % n].write(json.dumps(d) + "\n")
    for f, p in zip(fos, paths):
        f.close()
        os.replace(p + ".tmp", p)
    return paths


def digest_dir(out, name):
    lines = []
    for f in sorted(glob.glob(os.path.join(out, "**", f"{name}-*.data.json*"), recursive=True)):
        op = L.gzip.open if f.endswith(".gz") else open
        with op(f, "rt") as fh:
            lines.extend(fh)
    return L.digest_lines(lines)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engines", default="es710,es819")
    ap.add_argument("--heaps", default="256m,384m")
    ap.add_argument("--datasets", default="pmc,fat")
    ap.add_argument("--configs", default="2x5000,4x2500,8x1000")
    ap.add_argument("--indices", type=int, default=8)
    ap.add_argument("--port", type=int, default=19414)
    ap.add_argument("--run-id")
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    engines = [e for e in a.engines.split(",") if e]
    heaps = [h for h in a.heaps.split(",") if h]
    sets = [d for d in a.datasets.split(",") if d]
    configs = [c for c in a.configs.split(",") if c]
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + "-workerpage"
    lw = os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load")
    work = os.path.join(lw, run_id)
    os.makedirs(work, mode=0o700, exist_ok=True)
    resdir = os.path.join(ROOT, "results", "load", run_id)
    os.makedirs(resdir, exist_ok=True)
    tool, multi = R.tool_path("searchdump"), R.tool_path("searchdump-multi")
    data_dir = os.path.join(lw, "lowheap-data")
    os.makedirs(data_dir, exist_ok=True)
    src = {}
    for d in sets:
        p = os.path.join(data_dir, f"{d}.json")
        if not os.path.exists(p):
            {"pmc": L.gen_pmc, "fat": L.gen_fat}[d](p)
        n, dg = L.digest_file(p)
        src[d] = dict(path=p, docs=n, digest=dg, parts=split_dataset(p, a.indices, data_dir, d))
    res = dict(schema="edrs-workerpage/v1", run_id=run_id, indices=a.indices, configs=configs,
               tool=subprocess.run([tool, "--version"], capture_output=True, text=True).stdout.strip(),
               bin_dir=os.path.dirname(tool),
               started=datetime.datetime.now().astimezone().isoformat(timespec="seconds"), rows=[])

    for e in engines:
        for h in heaps:
            eng = L.Engine(e, h, work, a.port)
            cw = os.path.join(work, f"{e}-{h}")
            os.makedirs(cw, exist_ok=True)
            try:
                ver = eng.up()
                cl = eng.cl

                def seed(d):
                    for i, part in enumerate(src[d]["parts"]):
                        idx = f"{d}-{i}"
                        cl.req("DELETE", f"/{idx}", ok404=True)
                        cl.req("PUT", f"/{idx}", L.MAPPING)
                        rc, _, _, err = L.run_tool([tool, "--input", part, "--output", f"{eng.url}/{idx}", "--type=data",
                                                    f"--limit={200 if d == 'pmc' else 20}"],
                                                   os.path.join(cw, f"seed-{idx}.log"), cw)
                        if rc != 0:
                            raise RuntimeError(f"seed {idx}: exit {rc} {err}")
                    cl.req("POST", f"/{d}-*/_refresh")

                for d in sets:
                    seed(d)
                    for c in configs:
                        if c in ("auto", "off", "min"):
                            sizing, p, lim = [f"--serverLimits={c}"], None, None
                        else:
                            p, lim = (int(x) for x in c.split("x"))
                            sizing = []
                        tag = f"{d}-{c}"
                        out = os.path.join(cw, f"dump-{tag}")
                        shutil.rmtree(out, ignore_errors=True)
                        argv = [multi, "--input", eng.url, "--output", out, "--match", f"^{d}-[0-9]+$", *sizing]
                        if p:
                            argv += [f"--parallel={p}", "--", f"--limit={lim}"]
                        tmp = os.path.join(cw, f"tmp-dump-{tag}")
                        shutil.rmtree(tmp, ignore_errors=True)
                        os.makedirs(tmp)
                        rc, wall, st, err = L.run_tool(argv, os.path.join(cw, f"dump-{tag}.log"), tmp)
                        alive = eng.alive()
                        docs, dg = digest_dir(out, d) if rc == 0 else (None, None)
                        row = dict(engine=e, version=ver, heap=h, dataset=d, config=c, step="dump", exit=rc, wall_s=wall,
                                   node_alive=alive, equal=(docs, dg) == (src[d]["docs"], src[d]["digest"]), **st,
                                   error=err[:300])
                        res["rows"].append(row)
                        L.log(json.dumps(row))
                        if not alive:
                            eng.restart()
                            seed(d)
                        if rc == 0:
                            for i in range(a.indices):
                                cl.req("DELETE", f"/{d}-{i}", ok404=True)
                            largv = [multi, "--direction", "load", "--input", out, "--output", eng.url,
                                     "--match", f"^{d}-[0-9]+$", *sizing]
                            if p:
                                largv += [f"--parallel={p}", "--", f"--limit={lim}"]
                            tmp = os.path.join(cw, f"tmp-load-{tag}")
                            shutil.rmtree(tmp, ignore_errors=True)
                            os.makedirs(tmp)
                            rc2, wall2, st2, err2 = L.run_tool(largv, os.path.join(cw, f"load-{tag}.log"), tmp)
                            alive2 = eng.alive()
                            cnt = None
                            if alive2:
                                try:
                                    cl.req("POST", f"/{d}-*/_refresh")
                                    cnt = cl.req("GET", f"/{d}-*/_count")["count"]
                                except Exception:
                                    pass
                            row2 = dict(engine=e, version=ver, heap=h, dataset=d, config=c, step="load", exit=rc2,
                                        wall_s=wall2, node_alive=alive2, equal=cnt == src[d]["docs"], **st2,
                                        error=err2[:300])
                            res["rows"].append(row2)
                            L.log(json.dumps(row2))
                            if not alive2:
                                eng.restart()
                            if not alive2 or cnt != src[d]["docs"]:
                                seed(d)
                        shutil.rmtree(out, ignore_errors=True)
                    for i in range(a.indices):
                        cl.req("DELETE", f"/{d}-{i}", ok404=True)
            except Exception as ex:
                res["rows"].append(dict(engine=e, heap=h, error=f"cell error: {ex!r}"[:400]))
                L.log("cell error", e, h, repr(ex)[:300])
            finally:
                eng.down()
                if not a.keep:
                    subprocess.run((["rm", "-rf"] if os.geteuid() == 0 else ["podman", "unshare", "rm", "-rf"])
                                   + [eng.env["IT_DIR"], cw])
            res["finished"] = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
            json.dump(res, open(os.path.join(resdir, "workerpage.json"), "w"), indent=1)
    print("| engine | heap | dataset | config | dump exit / s | load exit / s | equal | 429 | breaker | retries | node |")
    print("|---|---|---|---|---|---|---|---|---|---|---|")
    keys = []
    for r in res["rows"]:
        if "config" in r and (r["engine"], r["heap"], r["dataset"], r["config"]) not in keys:
            keys.append((r["engine"], r["heap"], r["dataset"], r["config"]))
    for k in keys:
        dm = next((r for r in res["rows"] if r.get("step") == "dump" and (r["engine"], r["heap"], r["dataset"], r["config"]) == k), {})
        ld = next((r for r in res["rows"] if r.get("step") == "load" and (r["engine"], r["heap"], r["dataset"], r["config"]) == k), {})
        eq = "yes" if dm.get("equal") and ld.get("equal") else "NO"
        print(f"| {k[0]} {dm.get('version', '')} | {k[1]} | {k[2]} | {k[3]} | {dm.get('exit')} / {dm.get('wall_s')} | "
              f"{ld.get('exit', '-')} / {ld.get('wall_s', '-')} | {eq} | {dm.get('http429', 0)}+{ld.get('http429', 0)} | "
              f"{dm.get('circuit_breaking', 0)}+{ld.get('circuit_breaking', 0)} | {dm.get('retries', 0)}+{ld.get('retries', 0)} | "
              f"{'alive' if dm.get('node_alive', True) and ld.get('node_alive', True) else 'DOWN'} |")
    print(f"WORKERPAGE-DONE rows={len(res['rows'])} result=results/load/{run_id}/workerpage.json")
    return 0


if __name__ == "__main__":
    sys.exit(main())
