#!/usr/bin/env python3
"""LOAD-014 (L-809): many indices, many fields, many aliases (A part: 300 indices on one node).

    scripts/load/manyidx.py [--engine es717-plain] [--indices 300] [--docs 100] [--aliases 100]
                            [--fields 5000] [--heap 2048m] [--run-id ID] [--keep]

Provisions one engine (scripts/it/up.sh), creates `many-000`..`many-NNN` (edgen logs, 1 shard,
0 replicas, `--docs` documents each, seed = index number), `--aliases` aliases `a-000`.. over three
indices each (every 5th with a filter, every 7th with routing; one atomic POST /_aliases) and one
wide index `wide-NNNN` with a `--fields`-field mapping (edgen wide; total_fields.limit raised).
Then:

  dump      `many-*` as data, mapping, settings, index and alias; `wide-*` as data and mapping.
            Wall time and peak RSS (VmHWM, sampled) per run. Baseline: the meta types must
            finish within META_LIMIT_S each (recorded, so later runs compare against it).
  verify    edverify diff of `many-*` (and the wide index) against the data dumps.
  restore A delete everything, restore settings -> mapping -> alias -> data into the host-only
            URL; edverify diff; re-dump every meta type and compare it (generated keys removed)
            with the first dump.
  restore B delete everything, restore index (settings + mappings + aliases) -> data; same checks.
  leak      no scroll/PIT contexts, no .part files (LOAD-017).

Results: results/load/<run-id>/manyidx.json. Exit 0 when every assertion passed, 1 otherwise,
3 skipped (resource guard). 1,000 indices x many fields is Phase B (big host).
"""
import argparse, datetime, fcntl, json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run as R  # noqa: E402

ROOT = R.ROOT
META_LIMIT_S = 300
GENERATED = {"uuid", "creation_date", "provided_name", "version", "creation_date_string"}


def log(*a):
    print("manyidx:", *a, file=sys.stderr, flush=True)


def run_sampled(argv):
    """Run, sampling the child's VmHWM every 10 ms (short meta runs; see allocbench.run_sampled for why
    not ru_maxrss). Return (rc, wall_s, peak_kb, stderr)."""
    t0 = time.time()
    with open(os.devnull, "wb") as null:
        p = subprocess.Popen(argv, stdout=null, stderr=subprocess.PIPE)
        os.set_blocking(p.stderr.fileno(), False)
        err, hwm = b"", 0
        while True:
            try:
                with open(f"/proc/{p.pid}/status") as fh:
                    for line in fh:
                        if line.startswith("VmHWM:"):
                            hwm = max(hwm, int(line.split()[1]))
            except (FileNotFoundError, ProcessLookupError):
                pass
            try:
                err += p.stderr.read() or b""
            except (BlockingIOError, TypeError):
                pass
            if p.poll() is not None:
                break
            time.sleep(0.01)
        try:
            err += p.stderr.read() or b""
        except (BlockingIOError, TypeError):
            pass
    return p.returncode, time.time() - t0, hwm, err.decode(errors="replace")


def strip(v):
    if isinstance(v, dict):
        return {k: strip(x) for k, x in v.items() if k not in GENERATED}
    if isinstance(v, list):
        return [strip(x) for x in v]
    return v


def canon(path):
    """A dump file as a sorted list of canonical JSON lines (generated settings removed)."""
    out = []
    with open(path) as f:
        for line in f:
            line = line.strip()
            if line:
                v = json.loads(line)
                if isinstance(v, str):  # elasticdump meta format: one JSON string literal per line
                    v = json.loads(v)
                out.append(json.dumps(strip(v), sort_keys=True))
    return sorted(out)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engine", default="es717-plain")
    ap.add_argument("--indices", type=int, default=300)
    ap.add_argument("--docs", type=int, default=100)
    ap.add_argument("--aliases", type=int, default=100)
    ap.add_argument("--fields", type=int, default=5000)
    ap.add_argument("--wide-docs", type=int, default=50)
    ap.add_argument("--heap", default="2048m")
    ap.add_argument("--run-id")
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()

    lock = open("/tmp/searchdump-it.lock", "w")
    if os.environ.get("IT_LOCK_HELD") != "1":
        fcntl.flock(lock, fcntl.LOCK_EX)
    heap_mb = int(a.heap.rstrip("m"))
    need = heap_mb * 2 + 512 + 1024
    if R.meminfo_avail_mb() < need:
        print(f"MANYIDX-SUMMARY SKIPPED (MemAvailable {R.meminfo_avail_mb()} MB < {need} MB)")
        return 3
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + "-manyidx"
    work = os.path.join(os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"), run_id)
    os.makedirs(work, mode=0o700, exist_ok=True)
    resdir = os.path.join(ROOT, "results", "load", run_id)
    os.makedirs(resdir, exist_ok=True)
    edgen, edverify, tool = R.tool_path("edgen"), R.tool_path("edverify"), R.tool_path("searchdump")
    base = {}
    for line in subprocess.run([os.path.join(ROOT, "scripts/it/matrix.sh"), a.engine],
                               check=True, capture_output=True, text=True).stdout.splitlines():
        k, v = line.split("=", 1)
        base[k] = v
    up_profile = base.pop("UP_PROFILE")
    env = dict(os.environ, **base, IT_DIR=os.path.join(work, "it"), IT_LOCK_HELD="1",
               RUN_ID=f"load{os.getpid()}many", ES_HEAP=a.heap, OS_HEAP=a.heap, PORT="19204",
               IT_CONTAINER_MEM=f"{heap_mb * 2 + 1024}m")
    wide = f"wide-{a.fields}"
    res = dict(schema="edrs-manyidx/v1", run_id=run_id, engine=a.engine, heap=a.heap, indices=a.indices,
               docs_per_index=a.docs, aliases=a.aliases, fields=a.fields, wide_docs=a.wide_docs,
               meta_limit_s=META_LIMIT_S,
               git=subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"],
                                  capture_output=True, text=True).stdout.strip(),
               tool=subprocess.run([tool, "--version"], capture_output=True, text=True).stdout.strip(),
               started=datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
               runs=[], checks={}, failures=[])
    rc = 1

    def fail(msg):
        res["failures"].append(msg)
        log("FAIL", msg)

    try:
        log(f"provisioning {a.engine} (heap {a.heap})")
        subprocess.run([os.path.join(ROOT, "scripts/it/up.sh"), up_profile], env=env, check=True,
                       stdout=subprocess.DEVNULL)
        url = R.sh_env(os.path.join(env["IT_DIR"], "env"))["IT_ES_URL"]
        cl = R.Cluster(url)
        res["engine_version"] = cl.req("GET", "/")["version"]["number"]
        before = [i["index"] for i in cl.req("GET", "/_cat/indices?format=json&expand_wildcards=all")]
        flavour = R.engine_mapping_flavour(a.engine)
        names = [f"many-{i:03d}" for i in range(a.indices)]

        # seed
        t0 = time.monotonic()
        stored = 0
        for i, name in enumerate(names):
            s = R.seed_index(cl, edgen, dict(index=name, profile="logs", seed=i + 1, docs=a.docs), flavour)
            stored += s["stored"]
            if s["rejected"]:
                fail(f"seed {name}: {s['rejected']} rejected {s['reject_reasons']}")
        res["seed"] = dict(indices=len(names), docs=stored, seconds=round(time.monotonic() - t0, 1))
        log(f"seeded {len(names)} indices, {stored} docs in {res['seed']['seconds']} s")
        actions = []
        for j in range(a.aliases):
            for k in range(3):
                add = {"index": names[(j * 3 + k) % len(names)], "alias": f"a-{j:03d}"}
                if j % 5 == 0:
                    add["filter"] = {"term": {"log.level": "ERROR"}}
                if j % 7 == 0:
                    add["routing"] = "1"
                actions.append({"add": add})
        cl.req("POST", "/_aliases", {"actions": actions})
        m = json.loads(subprocess.run([edgen, "mapping", "--profile", "wide", "--engine", flavour, "--fields",
                                       str(a.fields)], check=True, capture_output=True).stdout)
        cl.req("PUT", "/" + wide, m)
        p = subprocess.run([edgen, "gen", "--profile", "wide", "--fields", str(a.fields), "--count",
                            str(a.wide_docs), "--format", "bulk", "--index", wide],
                           check=True, capture_output=True)
        for r in R.seed_bulk(cl, p.stdout.splitlines(keepends=True)):
            if r.get("status", 200) >= 300:
                fail(f"seed {wide}: {r}")
        cl.req("POST", f"/{wide}/_refresh")
        res["seed"]["wide_docs"] = cl.req("GET", f"/{wide}/_count")["count"]
        res["seed"]["wide_fields"] = len(m["mappings"]["properties"])
        res["seed"]["aliases"] = len({x for x in cl.req("GET", "/many-*/_alias/*").values()
                                      for x in x.get("aliases", {})})
        res["seed"]["shards"] = len(cl.req("GET", "/_cat/shards?format=json"))
        log(f"seed: {res['seed']}")

        def run(tag, args, meta=False, probe=False):
            out = os.path.join(work, tag)
            code, wall, hwm, err = run_sampled([tool, *args])
            row = dict(run=tag, args=[x.replace(url, "$URL").replace(work, "$WORK") for x in args], exit=code,
                       wall_s=round(wall, 2), peak_rss_mb=round(hwm / 1024, 1))
            if code:
                row["err"] = err.strip().splitlines()[-3:]
                if not probe:
                    fail(f"{tag}: exit {code}: {row['err']}")
            if meta and wall > META_LIMIT_S:
                fail(f"{tag}: {wall:.0f} s > {META_LIMIT_S} s")
            log(f"{tag}: exit {code}, {row['wall_s']} s, peak RSS {row['peak_rss_mb']} MB")
            res["runs"].append(row)
            return out

        def dump(tag, pattern, ty):
            out = os.path.join(work, f"{tag}.json")
            run(tag, ["--input", f"{url}/{pattern}", "--output", out, f"--type={ty}"], meta=ty != "data")
            return out

        def restore(tag, f, ty):
            args = ["--input", f, "--output", url, f"--type={ty}"]
            if ty == "data":
                args += ["--limit=1000", "--concurrency=4"]
            run(tag, args, meta=ty != "data")

        def verify(tag, pattern, files):
            rep = os.path.join(resdir, f"verify-{tag}.json")
            # one slice: 300 single-shard indices x 4 slices would pass ES's 500 open scroll contexts
            q = subprocess.run([edverify, "diff", "--slices", "1", "--a", f"{url}/{pattern}", "--b", *files,
                                "--out", rep],
                               capture_output=True, text=True)
            ok = q.returncode == 0
            res["checks"][f"digest-{tag}"] = ok
            if not ok:
                fail(f"digest {tag}: {(q.stdout + q.stderr)[-300:]}")

        def wipe():
            cl.req("DELETE", "/many-*,wide-*")
            left = cl.req("GET", "/_cat/indices/many-*,wide-*?format=json", ok404=True) or []
            if left:
                fail(f"wipe left {len(left)} indices")

        meta = ["mapping", "settings", "index", "alias"]
        f = {ty: dump(f"dump-{ty}", "many-*", ty) for ty in ["data", *meta]}
        f["wide-data"] = dump("dump-wide-data", wide, "data")
        f["wide-mapping"] = dump("dump-wide-mapping", wide, "mapping")
        f["wide-settings"] = dump("dump-wide-settings", wide, "settings")
        with open(f["data"]) as fh:
            res["checks"]["dump_docs"] = sum(1 for _ in fh)
        if res["checks"]["dump_docs"] != stored:
            fail(f"data dump has {res['checks']['dump_docs']} docs, cluster {stored}")
        for ty in meta:
            n = len(canon(f[ty]))
            res["checks"][f"{ty}_lines"] = n
        verify("source", "many-*", [f["data"]])
        verify("source-wide", wide, [f["wide-data"]])

        # probes: a PIT dump opens no scroll contexts; --slices 2 needs 2 per shard (600 > ES's default 500)
        pit = os.path.join(work, "dump-pit.json")
        run("probe-pit", ["--input", f"{url}/many-*", "--output", pit, "--type=data", "--searchType=pit"])
        verify("pit", "many-*", [pit])
        run("probe-slices2", ["--input", f"{url}/many-*", "--output", os.path.join(work, "dump-s2.json"),
                              "--type=data", "--slices=2"], probe=True)
        s2 = res["runs"][-1]
        res["checks"]["slices2"] = dict(exit=s2["exit"], err=s2.get("err"))
        if s2["exit"] == 0:
            verify("slices2", "many-*", [os.path.join(work, "dump-s2.json")])
        elif s2["exit"] != 1 or "--searchAfter --pit" not in " ".join(s2.get("err", [])):
            fail(f"probe-slices2: unclear failure {s2}")
        if os.path.exists(os.path.join(work, "dump-s2.json")):
            fail("probe-slices2: a failed dump left its output file")

        def compare_meta(rnd):
            for ty in meta:
                g = dump(f"{rnd}-redump-{ty}", "many-*", ty)
                same = canon(g) == canon(f[ty])
                res["checks"][f"{rnd}-meta-{ty}-equal"] = same
                if not same:
                    fail(f"{rnd}: re-dumped {ty} differs from the first dump")
            g = dump(f"{rnd}-redump-wide-mapping", wide, "mapping")
            same = canon(g) == canon(f["wide-mapping"])
            res["checks"][f"{rnd}-wide-mapping-equal"] = same
            if not same:
                fail(f"{rnd}: the {a.fields}-field mapping did not round-trip")

        # restore A: settings -> mapping -> alias -> data
        wipe()
        for ty in ["settings", "mapping", "alias", "data"]:
            restore(f"A-restore-{ty}", f[ty], ty)
        for ty in ["settings", "mapping", "data"]:
            restore(f"A-restore-wide-{ty}", f[f"wide-{ty}"], ty)
        cl.req("POST", "/many-*,wide-*/_refresh")
        verify("A", "many-*", [f["data"]])
        verify("A-wide", wide, [f["wide-data"]])
        compare_meta("A")

        # restore B: index -> data
        wipe()
        restore("B-restore-index", f["index"], "index")
        restore("B-restore-data", f["data"], "data")
        for ty in ["settings", "mapping", "data"]:
            restore(f"B-restore-wide-{ty}", f[f"wide-{ty}"], ty)
        cl.req("POST", "/many-*,wide-*/_refresh")
        verify("B", "many-*", [f["data"]])
        verify("B-wide", wide, [f["wide-data"]])
        compare_meta("B")

        expected = set(names) | {wide}
        res["leak"], lf = R.leak_check(cl, work, before=before, expected=expected)
        for x in lf:
            fail(x)
        rc = 0 if not res["failures"] else 1
    except Exception as e:
        res["failures"].append(f"error: {e!r}")
        log("ERROR", repr(e))
        rc = 1
    finally:
        res["finished"] = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
        res["pass"] = rc == 0
        json.dump(res, open(os.path.join(resdir, "manyidx.json"), "w"), indent=1)
        if not a.keep:
            subprocess.run(["podman", "rm", "-f", f"it-es-plain-{env['RUN_ID']}"], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
            subprocess.run((["rm", "-rf"] if os.geteuid() == 0 else ["podman", "unshare", "rm", "-rf"]) + [work])
    print(f"MANYIDX-SUMMARY engine={a.engine} indices={a.indices} aliases={a.aliases} fields={a.fields} "
          f"{'PASS' if rc == 0 else 'FAIL'} runs={len(res['runs'])} result=results/load/{run_id}/manyidx.json")
    for x in res["failures"]:
        print("  failure:", x)
    return rc


if __name__ == "__main__":
    sys.exit(main())
