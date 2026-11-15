#!/usr/bin/env python3
"""LOAD-LM (L-818): low-memory engines with searchdump-multi default data flags (1.1-10).

    scripts/load/lowheap.py [--engines es710,es861,es819,es912] [--heaps 256m,384m]
                            [--datasets pmc,fat] [--regression] [--pmc FILE]
                            [--run-id ID] [--port 19413] [--keep]

For every engine x heap it provisions one single-node engine with scripts/it/up.sh (ES_HEAP) and,
for each dataset:

  seed      the source file into index <dataset> with plain searchdump in small batches
            (--limit=200 pmc, 20 fat; not under test)
  dump      searchdump-multi --match ^<dataset>$ with NO extra flags (the default data flags);
            the dump file's (count, digest) must equal the source file's
  load      the index is deleted and searchdump-multi --direction load restores it from that dump
            directory with NO extra flags; read back with searchdump --limit=50, (count, digest)
            must equal the source
  node      after each step the engine must still answer (no OutOfMemoryError exit, container
            running), and no search context may stay open

Pass: every dump and load exits 0 (so no 429 retries were exhausted), counts and digests equal,
the node survives. Retries ("retrying in"), HTTP 429 and breaker trips are recorded, not failed.

--regression (optional) then repeats dump and load with the pre-1.1-10 flags
(`-- --limit=5000 --maxResponseMb=0`, load `-- --limit=5000 --maxBulkBytes=100mb`) and records
whether each failed (non-zero exit or node down) - expected on the small heaps, but not asserted:
some combinations (384m, pmc) pass with the old flags as well. A dead node is restarted.

Engines (scripts/it/matrix.sh keys):
  es710  bitnamilegacy elasticsearch 7.10.2       (es710-plain)
  es861  bitnamilegacy elasticsearch 8.6.1        (elasticsearch8-plain)
  es819  docker.elastic.co elasticsearch 8.19.23  (es819-plain)
  es912  bitnamilegacy elasticsearch 9.1.2        (elasticsearch-plain)

Datasets (generated, deterministic; cached under $LOAD_WORK/lowheap-data):
  pmc  20,000 article-like documents, ~22 KB of word text each (--pmc FILE uses a searchdump-format
       file instead, e.g. the Rally pmc corpus converted by `edgen convert --kind rally`)
  fat  3,000 documents of 60-100 KB of base64 noise (~80 KB, incompressible)

Mapping: one shard, no replica, `dynamic: false` with only `n` (long) mapped, so the heap is spent on
search responses and bulk requests, not on indexing text (as in the 1.1-10 reproduction).

Results: results/load/<run-id>/lowheap.json and a table on stdout. Exit 0 when every default-flag
check passed, 1 otherwise, 3 skipped (not enough memory).
"""
import argparse, datetime, fcntl, gzip, glob, hashlib, json, os, random, re, shutil, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run as R  # noqa: E402

ROOT = R.ROOT
ENGINES = {"es710": "es710-plain", "es861": "elasticsearch8-plain", "es819": "es819-plain",
           "es912": "elasticsearch-plain"}
OLD_DUMP = ["--limit=5000", "--maxResponseMb=0"]
OLD_LOAD = ["--limit=5000", "--maxBulkBytes=100mb"]
MAPPING = {"settings": {"number_of_shards": 1, "number_of_replicas": 0},
           "mappings": {"dynamic": False, "properties": {"n": {"type": "long"}}}}


def log(*a):
    print("lowheap:", *a, file=sys.stderr, flush=True)


# ---------------------------------------------------------------- datasets
def gen_pmc(path, docs=20000, seed=1110):
    rnd = random.Random(seed)
    vocab = ["".join(rnd.choice("abcdefghijklmnopqrstuvwxyz") for _ in range(rnd.randint(2, 11)))
             for _ in range(30000)]
    with open(path + ".tmp", "w") as fo:
        for i in range(docs):
            target = rnd.randint(12000, 32000)
            words, size = [], 0
            while size < target:
                w = rnd.choice(vocab)
                words.append(w)
                size += len(w) + 1
            body = " ".join(words)
            src = {"n": i, "title": " ".join(words[:12]), "journal": rnd.choice(vocab),
                   "year": 1990 + i % 35, "body": body}
            fo.write(json.dumps({"_index": "pmc", "_id": str(i), "_source": src}) + "\n")
    os.replace(path + ".tmp", path)


def gen_fat(path, docs=3000, seed=1111):
    import base64
    rnd = random.Random(seed)
    with open(path + ".tmp", "w") as fo:
        for i in range(docs):
            n = rnd.randint(45000, 75000)  # base64 -> 60-100 KB
            text = base64.b64encode(rnd.randbytes(n)).decode()
            fo.write(json.dumps({"_index": "fat", "_id": str(i), "_source": {"n": i, "bytes": n, "text": text}}) + "\n")
    os.replace(path + ".tmp", path)


def page_per(note):
    m = re.search(r"dumping (\d+) per page", note or "")
    return m.group(1) if m else "-"


def digest_lines(lines):
    h = []
    for line in lines:
        if not line.strip():
            continue
        d = json.loads(line)
        h.append(d["_id"] + hashlib.sha1(json.dumps(d["_source"], sort_keys=True).encode()).hexdigest())
    h.sort()
    return len(h), hashlib.sha256("\n".join(h).encode()).hexdigest()[:16]


def digest_file(path):
    op = gzip.open if path.endswith(".gz") else open
    with op(path, "rt") as fh:
        return digest_lines(fh)


# ---------------------------------------------------------------- engine
class Engine:
    def __init__(self, key, heap, work, port):
        self.key, self.heap, self.port = key, heap, port
        base = {}
        for line in subprocess.run([os.path.join(ROOT, "scripts/it/matrix.sh"), ENGINES[key]],
                                   check=True, capture_output=True, text=True).stdout.splitlines():
            k, v = line.split("=", 1)
            base[k] = v
        self.profile = base.pop("UP_PROFILE")
        heap_mb = int(heap.rstrip("m"))
        self.run_id = f"lm{os.getpid()}{key}{heap_mb}"
        self.name = f"it-es-plain-{self.run_id}"
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(("IT_", "ES_", "OS_")) and k not in ("RUN_ID", "PORT", "UP_PROFILE")}
        self.env = dict(env, **base, IT_DIR=os.path.join(work, f"it-{key}-{heap_mb}"), IT_LOCK_HELD="1",
                        RUN_ID=self.run_id, ES_HEAP=heap, OS_HEAP=heap, PORT=str(port),
                        IT_CONTAINER_MEM=f"{heap_mb * 2 + 1024}m")
        self.url = self.cl = None

    def up(self):
        subprocess.run([os.path.join(ROOT, "scripts/it/up.sh"), self.profile], env=self.env, check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.url = R.sh_env(os.path.join(self.env["IT_DIR"], "env"))["IT_ES_URL"]
        self.cl = R.Cluster(self.url)
        return self.cl.req("GET", "/")["version"]["number"]

    def state(self):
        p = subprocess.run(["podman", "inspect", self.name, "--format",
                            "{{.State.Status}} {{.State.ExitCode}} {{.State.OOMKilled}}"],
                           capture_output=True, text=True)
        return p.stdout.strip() or p.stderr.strip()[:200]

    def alive(self, wait_s=20):
        deadline = time.time() + wait_s
        while time.time() < deadline:
            try:
                h = self.cl.req("GET", "/_cluster/health?wait_for_status=yellow&timeout=5s", retry_429=False)
                if h.get("status") in ("green", "yellow"):
                    return True
            except Exception:
                pass
            if not self.state().startswith("running"):
                return False
            time.sleep(2)
        return False

    def restart(self):
        subprocess.run(["podman", "start", self.name], capture_output=True)
        deadline = time.time() + 180
        while time.time() < deadline:
            if self.alive(5):
                return True
            time.sleep(3)
        return False

    def oom_lines(self):
        p = subprocess.run(["podman", "logs", self.name], capture_output=True, text=True)
        # not the JVM's own -XX:+HeapDumpOnOutOfMemoryError option line
        return sum(1 for x in (p.stdout + p.stderr).splitlines() if "java.lang.OutOfMemoryError" in x)

    def breakers(self):
        try:
            n = self.cl.req("GET", "/_nodes/stats/breaker", retry_429=False)["nodes"].popitem()[1]["breakers"]
            return {k: v["tripped"] for k, v in n.items() if v["tripped"]}
        except Exception as e:
            return {"error": repr(e)[:120]}

    def down(self):
        subprocess.run(["podman", "rm", "-f", self.name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


# ---------------------------------------------------------------- runs
def run_tool(argv, logf, tmpdir):
    t0 = time.time()
    env = dict(os.environ, TMPDIR=tmpdir)
    with open(logf, "wb") as fo:
        rc = subprocess.run(argv, stdout=fo, stderr=subprocess.STDOUT, env=env).returncode
    wall = round(time.time() - t0, 1)
    text = open(logf, errors="replace").read()
    for f in glob.glob(os.path.join(tmpdir, "**", "*.log"), recursive=True):
        text += open(f, errors="replace").read()
    stats = {"retries": text.count("retrying in"), "http429": text.count("429"),
             "circuit_breaking": text.count("circuit_breaking_exception")}
    errs = [x for x in text.splitlines() if x.lstrip().startswith(("Error", "error:"))]
    return rc, wall, stats, (errs[-1] if errs else "")[:400]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engines", default="es710,es861,es819,es912")
    ap.add_argument("--heaps", default="256m,384m")
    ap.add_argument("--datasets", default="pmc,fat")
    ap.add_argument("--regression", action="store_true", help="also run the pre-1.1-10 flags (expected to fail)")
    ap.add_argument("--pmc", help="searchdump-format pmc file instead of the generated one")
    ap.add_argument("--run-id")
    ap.add_argument("--port", type=int, default=19413)
    ap.add_argument("--keep", action="store_true", help="keep the work directory (dumps, logs)")
    a = ap.parse_args()
    engines = [e for e in a.engines.split(",") if e]
    heaps = [h for h in a.heaps.split(",") if h]
    sets = [d for d in a.datasets.split(",") if d]
    for e in engines:
        if e not in ENGINES:
            sys.exit(f"lowheap: unknown engine {e} (known: {', '.join(ENGINES)})")

    lock = open("/tmp/searchdump-it.lock", "w")
    if os.environ.get("IT_LOCK_HELD") != "1":
        fcntl.flock(lock, fcntl.LOCK_EX)
    need = max(int(h.rstrip("m")) for h in heaps) * 2 + 1024 + 1024
    if R.meminfo_avail_mb() < need:
        print(f"LOWHEAP-SUMMARY SKIPPED (MemAvailable {R.meminfo_avail_mb()} MB < {need} MB)")
        return 3
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + "-lowheap"
    lw = os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load")
    work = os.path.join(lw, run_id)
    os.makedirs(work, mode=0o700, exist_ok=True)
    resdir = os.path.join(ROOT, "results", "load", run_id)
    os.makedirs(resdir, exist_ok=True)
    tool, multi = R.tool_path("searchdump"), R.tool_path("searchdump-multi")
    res = dict(schema="edrs-lowheap/v1", run_id=run_id, engines=engines, heaps=heaps, datasets={},
               regression=a.regression,
               git=subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"],
                                  capture_output=True, text=True).stdout.strip(),
               tool=subprocess.run([tool, "--version"], capture_output=True, text=True).stdout.strip(),
               started=datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
               cells=[], failures=[])

    # datasets
    data_dir = os.path.join(lw, "lowheap-data")
    os.makedirs(data_dir, exist_ok=True)
    src = {}
    for d in sets:
        p = a.pmc if (d == "pmc" and a.pmc) else os.path.join(data_dir, f"{d}.json")
        if not os.path.exists(p):
            log(f"generating {p}")
            {"pmc": gen_pmc, "fat": gen_fat}[d](p)
        n, dg = digest_file(p)
        src[d] = dict(path=p, docs=n, digest=dg)
        res["datasets"][d] = dict(docs=n, avg_kb=round(os.path.getsize(p) / max(1, n) / 1024, 1), digest=dg,
                                  file=os.path.basename(p))
    log(f"datasets: {res['datasets']}")

    def fail(cell, msg):
        cell["failures"].append(msg)
        res["failures"].append(f"{cell['engine']}/{cell['heap']}: {msg}")
        log("FAIL", cell["engine"], cell["heap"], msg)

    rc = 1
    try:
        for e in engines:
            for h in heaps:
                eng = Engine(e, h, work, a.port)
                cell = dict(engine=e, heap=h, steps=[], failures=[], regression=[])
                res["cells"].append(cell)
                cw = os.path.join(work, f"{e}-{h}")
                os.makedirs(cw, exist_ok=True)
                try:
                    log(f"== {e} heap {h}: up")
                    cell["version"] = eng.up()
                    cl = eng.cl

                    def step(tag, argv, kind):
                        tmp = os.path.join(cw, f"tmp-{tag}")
                        shutil.rmtree(tmp, ignore_errors=True)
                        os.makedirs(tmp)
                        rc_, wall, st, err = run_tool(argv, os.path.join(cw, f"{tag}.log"), tmp)
                        alive = eng.alive()
                        row = dict(step=tag, kind=kind, exit=rc_, wall_s=wall, node_alive=alive, **st, error=err)
                        log(f"{e}/{h} {tag}: exit {rc_} {wall}s node_alive={alive} retries={st['retries']}"
                            + (f" error: {err[:160]}" if rc_ else ""))
                        return row

                    for d in sets:
                        s = src[d]
                        cl.req("DELETE", f"/{d}", ok404=True)
                        cl.req("PUT", f"/{d}", MAPPING)
                        r = step(f"seed-{d}", [tool, "--input", s["path"], "--output", f"{eng.url}/{d}",
                                               "--type=data", f"--limit={200 if d == 'pmc' else 20}"], "seed")
                        cell["steps"].append(r)
                        if r["exit"] != 0:
                            fail(cell, f"seed {d}: exit {r['exit']}")
                            continue
                        cl.req("POST", f"/{d}/_refresh")
                        # dump with the default data flags
                        out = os.path.join(cw, f"dump-{d}")
                        r = step(f"dump-{d}", [multi, "--input", eng.url, "--output", out, "--match", f"^{d}$"], "dump")
                        f = glob.glob(os.path.join(out, f"{d}.data.json*"))
                        r["docs"], r["digest"] = digest_file(f[0]) if f and r["exit"] == 0 else (None, None)
                        note = [x for x in open(os.path.join(cw, f"dump-{d}.log"), errors="replace")
                                if "per page" in x]
                        r["page_note"] = note[-1].strip()[:200] if note else ""
                        r["equal"] = (r["docs"], r["digest"]) == (s["docs"], s["digest"])
                        cell["steps"].append(r)
                        for bad, msg in ((r["exit"] != 0, f"dump {d}: exit {r['exit']}"),
                                         (not r["node_alive"], f"dump {d}: node down ({eng.state()})"),
                                         (r["exit"] == 0 and not r["equal"],
                                          f"dump {d}: {r['docs']} docs digest {r['digest']} != source")):
                            if bad:
                                fail(cell, msg)
                        if not r["node_alive"]:
                            eng.restart()
                        if r["exit"] != 0:
                            continue
                        # load with the default data flags
                        cl.req("DELETE", f"/{d}", ok404=True)
                        r = step(f"load-{d}", [multi, "--direction", "load", "--input", out, "--output", eng.url,
                                               "--match", f"^{d}$"], "load")
                        if r["exit"] == 0 and r["node_alive"]:
                            cl.req("POST", f"/{d}/_refresh")
                            back = os.path.join(cw, f"readback-{d}.json")
                            q = run_tool([tool, "--input", f"{eng.url}/{d}", "--output", back, "--type=data",
                                          "--limit=50"], os.path.join(cw, f"readback-{d}.log"), cw)
                            r["docs"], r["digest"] = digest_file(back) if q[0] == 0 else (None, None)
                            os.remove(back) if os.path.exists(back) else None
                        r["equal"] = (r.get("docs"), r.get("digest")) == (s["docs"], s["digest"])
                        cell["steps"].append(r)
                        for bad, msg in ((r["exit"] != 0, f"load {d}: exit {r['exit']}"),
                                         (not r["node_alive"], f"load {d}: node down ({eng.state()})"),
                                         (r["exit"] == 0 and not r["equal"],
                                          f"load {d}: read back {r.get('docs')} docs digest {r.get('digest')} != source")):
                            if bad:
                                fail(cell, msg)
                        if not r["node_alive"]:
                            eng.restart()

                        if a.regression:
                            ro = os.path.join(cw, f"olddump-{d}")
                            r = step(f"old-dump-{d}", [multi, "--input", eng.url, "--output", ro, "--match", f"^{d}$",
                                                       "--", *OLD_DUMP], "old-dump")
                            r["failed_as_expected"] = r["exit"] != 0 or not r["node_alive"]
                            cell["regression"].append(r)
                            if not r["node_alive"]:
                                r["restarted"] = eng.restart()
                            shutil.rmtree(ro, ignore_errors=True)
                            if eng.alive():
                                cl.req("DELETE", f"/{d}", ok404=True)
                                r = step(f"old-load-{d}", [multi, "--direction", "load", "--input", out,
                                                           "--output", eng.url, "--match", f"^{d}$", "--", *OLD_LOAD],
                                         "old-load")
                                r["failed_as_expected"] = r["exit"] != 0 or not r["node_alive"]
                                cell["regression"].append(r)
                                if not r["node_alive"]:
                                    r["restarted"] = eng.restart()
                        shutil.rmtree(out, ignore_errors=True)
                        if eng.alive():
                            cl.req("DELETE", f"/{d}", ok404=True)
                    cell["oom_log_lines"] = eng.oom_lines()
                    if eng.alive():
                        cell["breakers_tripped"] = eng.breakers()
                        leak, lf = R.leak_check(cl, None)
                        cell["leak"] = leak
                        for x in lf:
                            fail(cell, x)
                    cell["container"] = eng.state()
                except Exception as ex:
                    fail(cell, f"error: {ex!r}"[:400])
                finally:
                    eng.down()
                    if not a.keep:
                        subprocess.run((["rm", "-rf"] if os.geteuid() == 0 else ["podman", "unshare", "rm", "-rf"])
                                       + [eng.env["IT_DIR"], cw])
        rc = 0 if not res["failures"] else 1
    finally:
        res["finished"] = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
        res["pass"] = rc == 0
        json.dump(res, open(os.path.join(resdir, "lowheap.json"), "w"), indent=1)
        if not a.keep:
            subprocess.run((["rm", "-rf"] if os.geteuid() == 0 else ["podman", "unshare", "rm", "-rf"]) + [work])

    print(f"| engine | heap | dataset | dump (s) | load (s) | equal | retries | node | docs/page |")
    print("|---|---|---|---|---|---|---|---|---|")
    for c in res["cells"]:
        for d in sets:
            dm = next((x for x in c["steps"] if x["step"] == f"dump-{d}"), {})
            ld = next((x for x in c["steps"] if x["step"] == f"load-{d}"), {})
            ok = "yes" if dm.get("equal") and ld.get("equal") else "NO"
            print(f"| {c['engine']} {c.get('version', '?')} | {c['heap']} | {d} | {dm.get('exit')} / {dm.get('wall_s')} "
                  f"| {ld.get('exit')} / {ld.get('wall_s')} | {ok} | {dm.get('retries', 0) + ld.get('retries', 0)} "
                  f"| {'alive' if all(x.get('node_alive', True) for x in (dm, ld)) else 'DOWN'} | "
                  f"{page_per(dm.get('page_note'))} |")
    if a.regression:
        print("\n| engine | heap | old-flag run | exit | node | failed as expected |")
        print("|---|---|---|---|---|---|")
        for c in res["cells"]:
            for r in c["regression"]:
                print(f"| {c['engine']} | {c['heap']} | {r['step']} | {r['exit']} | "
                      f"{'alive' if r['node_alive'] else 'DOWN'} | {'yes' if r['failed_as_expected'] else 'no'} |")
    print(f"LOWHEAP-SUMMARY {'PASS' if rc == 0 else 'FAIL'} cells={len(res['cells'])} "
          f"result=results/load/{run_id}/lowheap.json")
    for x in res["failures"]:
        print("  failure:", x)
    return rc


if __name__ == "__main__":
    sys.exit(main())
