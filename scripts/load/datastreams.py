#!/usr/bin/env python3
"""LOAD-018 (L-812): data streams at scale.

    scripts/load/datastreams.py [--engine es717-plain] [--docs 1000000] [--heap 2048m] [--run-id ID] [--keep]

Provisions one engine (scripts/it/up.sh), adds a composable template (`ds-l812-*`, data_stream, edgen logs
mapping, 1 shard, 0 replicas) and generates `--docs` edgen logs documents (seed 1) into a file. Then:

  index      a restore into the stream with the default --bulkAction index fails on every document,
             prints the --bulkAction create hint once and exits 3 (failed writes); nothing is stored.
  routed     1000 edgen documents with the default 10% routing into `ds-l812-r` with --bulkAction create:
             the routed ones are refused (no data_stream.allow_custom_routing), the routing hint is printed
             once, exit 3, and the rest are stored. The main dataset is generated with --routing-pct 0.
  restore    the file into data stream `ds-l812-a` with --bulkAction create (the stream rolls over
             once at half the documents, so it has two backing indices); edverify against the file.
  dump       `ds-l812-a` into a file: every `_index` is one of its `.ds-*` backing indices; edverify.
  host-only  the dump into the host-only URL: with the default --bulkAction index it stops before the
             first bulk (exit 1, names the stream and --bulkAction create); after the stream is deleted,
             --bulkAction create recreates `ds-l812-a` from the backing-index names (TST-BRK-XV-003);
             edverify against the original file.
  rename     the dump into `--output .../ds-l812-b` with --bulkAction create; edverify.
  leak       no scroll/PIT contexts, no .part files (LOAD-017).

Results: results/load/<run-id>/datastreams.json. Exit 0 when every assertion passed, 1 otherwise, 3 skipped.
"""
import argparse, datetime, fcntl, json, os, subprocess, sys, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run as R  # noqa: E402
from largedocs import run_sampled  # noqa: E402

ROOT = R.ROOT


def log(*a):
    print("datastreams:", *a, file=sys.stderr, flush=True)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engine", default="es717-plain")
    ap.add_argument("--docs", type=int, default=1_000_000)
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
        print(f"DATASTREAMS-SUMMARY SKIPPED (MemAvailable {R.meminfo_avail_mb()} MB < {need} MB)")
        return 3
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + "-datastreams"
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
               RUN_ID=f"load{os.getpid()}ds", ES_HEAP=a.heap, OS_HEAP=a.heap, PORT="19206",
               IT_CONTAINER_MEM=f"{heap_mb * 2 + 1024}m")
    res = dict(schema="edrs-datastreams/v1", run_id=run_id, engine=a.engine, heap=a.heap, docs=a.docs,
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
        src = os.path.join(work, "src.json")
        t0 = time.monotonic()
        # no routing: a data stream refuses routed documents (probed separately below)
        subprocess.run([edgen, "gen", "--profile", "logs", "--seed", "1", "--count", str(a.docs), "--format",
                        "elasticdump", "--index", "logs-src", "--routing-pct", "0", "--out", src],
                       check=True, capture_output=True)
        res["source"] = dict(bytes=os.path.getsize(src), seconds=round(time.monotonic() - t0, 1))
        log(f"generated {a.docs} docs, {res['source']['bytes'] / 1e6:.0f} MB")

        log(f"provisioning {a.engine} (heap {a.heap})")
        subprocess.run([os.path.join(ROOT, "scripts/it/up.sh"), up_profile], env=env, check=True,
                       stdout=subprocess.DEVNULL)
        url = R.sh_env(os.path.join(env["IT_DIR"], "env"))["IT_ES_URL"]
        cl = R.Cluster(url)
        res["engine_version"] = cl.req("GET", "/")["version"]["number"]
        before = [i["index"] for i in cl.req("GET", "/_cat/indices?format=json&expand_wildcards=all")]
        m = json.loads(subprocess.run([edgen, "mapping", "--profile", "logs", "--engine",
                                       R.engine_mapping_flavour(a.engine)], check=True, capture_output=True).stdout)
        cl.req("PUT", "/_index_template/l812", {"index_patterns": ["ds-l812-*"], "data_stream": {}, "priority": 500,
                                                "template": {"settings": m["settings"], "mappings": m["mappings"]}})

        def run(tag, args, want=0):
            code, wall, hwm, err = run_sampled([tool, *args])
            row = dict(run=tag, args=[x.replace(url, "$URL").replace(work, "$WORK") for x in args], exit=code,
                       want=want, wall_s=round(wall, 2), peak_rss_mb=round(hwm / 1024, 1))
            row["docs_per_s"] = round(a.docs / wall) if wall else None
            lines = err.strip().splitlines()
            row["error"] = next((x for x in reversed(lines) if x.startswith("Error")), "")[:600]
            row["hints"] = sum("--bulkAction create" in x for x in lines)
            row["routing_hints"] = sum("allow_custom_routing" in x for x in lines)
            row["summary"] = [x[-200:] for x in lines if "Total Writes" in x or "Failed Writes" in x][-2:]
            row["item_errors"] = sorted({x.split("]", 1)[-1][:300] for x in lines if "WARN" in x or "ERROR" in x})[:5]
            if code != want:
                fail(f"{tag}: exit {code}, want {want}: {lines[-3:]}")
            log(f"{tag}: exit {code} (want {want}), {row['wall_s']} s, peak RSS {row['peak_rss_mb']} MB"
                + (f", error: {row['error'][:200]}" if code else ""))
            res["runs"].append(row)
            return row

        def verify(tag, a_, b_):
            rep = os.path.join(resdir, f"verify-{tag}.json")
            q = subprocess.run([edverify, "diff", "--index-as", "x", "--a", a_, "--b", b_, "--out", rep],
                               capture_output=True, text=True)
            res["checks"][f"digest-{tag}"] = q.returncode == 0
            if q.returncode:
                fail(f"digest {tag}: {(q.stdout + q.stderr)[-300:]}")

        def stream(name):
            d = cl.req("GET", f"/_data_stream/{name}", ok404=True)
            ds = (d or {}).get("data_streams") or [{}]
            return [i["index_name"] for i in ds[0].get("indices", [])]

        def delete_stream(name):
            cl.req("DELETE", f"/_data_stream/{name}", ok404=True)

        # 1. the default --bulkAction index into a stream: every document fails, one hint (small slice)
        small = os.path.join(work, "small.json")
        with open(src, "rb") as fi, open(small, "wb") as fo:
            for i, line in enumerate(fi):
                if i == 1000:
                    break
                fo.write(line)
        r = run("index-into-stream", ["--input", small, "--output", f"{url}/ds-l812-a", "--type=data"], want=3)
        res["checks"]["index-hint-once"] = r["hints"] == 1
        cl.req("POST", "/ds-l812-a/_refresh", ok404=True)
        cnt = cl.req("GET", "/ds-l812-a/_count", ok404=True) or {}
        res["checks"]["index-stored-nothing"] = cnt.get("count", 0) == 0
        delete_stream("ds-l812-a")

        # 1b. routed documents (edgen's default 10%): refused by the stream, named once, the rest written
        routed = os.path.join(work, "routed.json")
        subprocess.run([edgen, "gen", "--profile", "logs", "--seed", "7", "--count", "1000", "--format",
                        "elasticdump", "--index", "logs-src", "--out", routed], check=True, capture_output=True)
        with open(routed) as fh:
            n_routed = sum(1 for x in fh if '"_routing"' in x or '"routing"' in x)
        r = run("routed-into-stream", ["--input", routed, "--output", f"{url}/ds-l812-r", "--type=data",
                                       "--bulkAction=create"], want=3)
        res["checks"]["routed"] = dict(docs_with_routing=n_routed, summary=r["summary"],
                                       hint_once=r["routing_hints"] == 1)
        cl.req("POST", "/ds-l812-r/_refresh")
        stored = cl.req("GET", "/ds-l812-r/_count")["count"]
        res["checks"]["routed"]["stored"] = stored
        if stored != 1000 - n_routed or not res["checks"]["routed"]["hint_once"]:
            fail(f"routed probe: {res['checks']['routed']}")
        delete_stream("ds-l812-r")

        # 2. restore into the stream with create, rolling over at half way
        rolled = {}

        def roll():
            while "at" not in rolled and not rolled.get("stop"):
                try:
                    st = cl.req("GET", "/ds-l812-a/_stats/indexing", ok404=True) or {}
                    n = st.get("_all", {}).get("primaries", {}).get("indexing", {}).get("index_total", 0)
                    if n >= a.docs // 2:
                        cl.req("POST", "/ds-l812-a/_rollover")
                        rolled["at"] = n
                        return
                except Exception as e:  # noqa: BLE001 - stream not created yet
                    rolled["last_error"] = repr(e)[:200]
                time.sleep(0.2)

        th = threading.Thread(target=roll, daemon=True)
        th.start()
        run("restore-create", ["--input", src, "--output", f"{url}/ds-l812-a", "--type=data", "--bulkAction=create",
                               "--limit=5000", "--concurrency=4", "--noRefresh"])
        rolled["stop"] = True
        th.join(5)
        cl.req("POST", "/ds-l812-a/_refresh")
        backing = stream("ds-l812-a")
        res["checks"]["rolled_over_at"] = rolled.get("at")
        res["checks"]["backing_indices"] = backing
        if len(backing) < 2:
            fail(f"expected two backing indices after the rollover, got {backing}")
        verify("restore-create", f"{url}/ds-l812-a", src)

        # 3. dump the stream
        dump = os.path.join(work, "dump.json")
        run("dump-stream", ["--input", f"{url}/ds-l812-a", "--output", dump, "--type=data", "--limit=5000"])
        seen, n = {}, 0
        with open(dump) as fh:
            for line in fh:
                ix = json.loads(line)["_index"]
                seen[ix] = seen.get(ix, 0) + 1
                n += 1
        res["checks"]["dump_docs"] = n
        res["checks"]["dump_index_values"] = seen
        if n != a.docs or set(seen) != set(backing):
            fail(f"dump: {n} docs, _index values {sorted(seen)} vs backing {backing}")
        verify("dump", f"{url}/ds-l812-a", dump)

        # 4. host-only restore of the dump
        r = run("host-only-index", ["--input", dump, "--output", url, "--type=data"], want=1)
        res["checks"]["host-only-index-names-stream"] = ("ds-l812-a" in r["error"]
                                                        and "--bulkAction create" in r["error"])
        if not res["checks"]["host-only-index-names-stream"]:
            fail("host-only --bulkAction index: unclear error")
        delete_stream("ds-l812-a")
        run("host-only-create", ["--input", dump, "--output", url, "--type=data", "--bulkAction=create",
                                 "--limit=5000", "--concurrency=4", "--noRefresh"])
        cl.req("POST", "/ds-l812-a/_refresh")
        res["checks"]["host-only-backing"] = stream("ds-l812-a")
        verify("host-only-create", f"{url}/ds-l812-a", src)
        stray = [i["index"] for i in cl.req("GET", "/_cat/indices/.ds-*?format=json&expand_wildcards=all")
                 if not i["index"].startswith(".ds-ds-l812-") and not i["index"].startswith(".ds-ilm")
                 and not i["index"].startswith(".ds-.logs")]
        res["checks"]["no-plain-ds-index"] = stray == []
        if stray:
            fail(f"plain indices named like backing indices: {stray}")

        # 5. into another stream by name
        run("rename-create", ["--input", dump, "--output", f"{url}/ds-l812-b", "--type=data", "--bulkAction=create",
                              "--limit=5000", "--concurrency=4", "--noRefresh"])
        cl.req("POST", "/ds-l812-b/_refresh")
        verify("rename-create", f"{url}/ds-l812-b", src)

        for k in ("index-hint-once", "index-stored-nothing"):
            if not res["checks"][k]:
                fail(f"check {k}")
        expected = set(stream("ds-l812-a")) | set(stream("ds-l812-b"))
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
        json.dump(res, open(os.path.join(resdir, "datastreams.json"), "w"), indent=1)
        if not a.keep:
            subprocess.run(["podman", "rm", "-f", f"it-es-plain-{env['RUN_ID']}"], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
            subprocess.run((["rm", "-rf"] if os.geteuid() == 0 else ["podman", "unshare", "rm", "-rf"]) + [work])
    print(f"DATASTREAMS-SUMMARY engine={a.engine} docs={a.docs} {'PASS' if rc == 0 else 'FAIL'} "
          f"runs={len(res['runs'])} result=results/load/{run_id}/datastreams.json")
    for x in res["failures"]:
        print("  failure:", x)
    return rc


if __name__ == "__main__":
    sys.exit(main())
