#!/usr/bin/env python3
"""LOAD-015 (L-810): large documents.

    scripts/load/largedocs.py [--engine es717-plain] [--pmc FILE] [--pmc-docs 20000] [--huge-docs 60]
                              [--heap 2048m] [--run-id ID] [--keep]

Provisions one engine (scripts/it/up.sh; `http.max_content_length` left at its 100 MB default) and runs:

  pmc        Rally pmc articles (`--pmc`, one source document per line; the first `--pmc-docs`),
             converted with `edgen convert --kind rally`; restore, edverify, dump, edverify.
  huge       edgen `huge` documents of 1-10 MB; restore (batches split at --maxBulkBytes), dump
             with a small --limit, edverify both ways.
  doc90      one 90 MB document near the 100 MB limit, restored with `--maxBulkBytes 95mb`
             (and with the default), dumped back; edverify.
  over-limit a 110 MB document: refused before sending by the default --maxDocMb 100 (exit 1, names
             the flag); with --maxDocMb 0 it reaches the cluster, which answers 413: exit 1 with a
             message naming http.max_content_length, not retried.
  bulk-413   two 60 MB documents with --maxBulkBytes 200mb (one 120 MB bulk): 413, exit 1, clear
             message naming --maxBulkBytes; with the default --maxBulkBytes the same file restores.
  leak       no scroll/PIT contexts, no .part files (LOAD-017).

Results: results/load/<run-id>/largedocs.json. Exit 0 when every assertion passed, 1 otherwise, 3 skipped.
"""
import argparse, datetime, fcntl, json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run as R  # noqa: E402

ROOT = R.ROOT
MB = 1 << 20


def log(*a):
    print("largedocs:", *a, file=sys.stderr, flush=True)


def run_sampled(argv):
    """Run, sampling the child's VmHWM every 10 ms. Return (rc, wall_s, peak_kb, stderr)."""
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


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--engine", default="es717-plain")
    ap.add_argument("--pmc", default="/var/tmp/searchdump-datasets/pmc-20000.json")
    ap.add_argument("--pmc-docs", type=int, default=20000)
    ap.add_argument("--huge-docs", type=int, default=60)
    ap.add_argument("--heap", default="2048m")
    ap.add_argument("--run-id")
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()

    lock = open("/tmp/searchdump-it.lock", "w")
    if os.environ.get("IT_LOCK_HELD") != "1":
        fcntl.flock(lock, fcntl.LOCK_EX)
    heap_mb = int(a.heap.rstrip("m"))
    need = heap_mb * 2 + 512 + 1536
    if R.meminfo_avail_mb() < need:
        print(f"LARGEDOCS-SUMMARY SKIPPED (MemAvailable {R.meminfo_avail_mb()} MB < {need} MB)")
        return 3
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + "-largedocs"
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
               RUN_ID=f"load{os.getpid()}large", ES_HEAP=a.heap, OS_HEAP=a.heap, PORT="19205",
               IT_CONTAINER_MEM=f"{heap_mb * 2 + 1024}m")
    res = dict(schema="edrs-largedocs/v1", run_id=run_id, engine=a.engine, heap=a.heap,
               git=subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"],
                                  capture_output=True, text=True).stdout.strip(),
               tool=subprocess.run([tool, "--version"], capture_output=True, text=True).stdout.strip(),
               started=datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
               datasets={}, runs=[], checks={}, failures=[])
    rc = 1

    def fail(msg):
        res["failures"].append(msg)
        log("FAIL", msg)

    try:
        # datasets (generated before the engine starts: less memory pressure)
        def gen_huge(name, count, lo, hi, seed):
            out = os.path.join(work, f"{name}.json")
            subprocess.run([edgen, "gen", "--profile", "huge", "--seed", str(seed), "--count", str(count),
                            "--huge-min-mb", str(lo), "--huge-max-mb", str(hi), "--index", name, "--out", out],
                           check=True, capture_output=True)
            res["datasets"][name] = dict(docs=count, bytes=os.path.getsize(out), min_mb=lo, max_mb=hi)
            return out

        pmc = os.path.join(work, "pmc.json")
        with open(pmc, "wb") as fo:
            src = subprocess.Popen(["head", "-n", str(a.pmc_docs), a.pmc], stdout=subprocess.PIPE)
            subprocess.run([edgen, "convert", "--kind", "rally", "--dataset", "pmc", "--index", "pmc"],
                           stdin=src.stdout, stdout=fo, check=True, stderr=subprocess.DEVNULL)
            src.wait()
        with open(pmc, "rb") as fh:
            n_pmc = sum(1 for _ in fh)
        res["datasets"]["pmc"] = dict(docs=n_pmc, bytes=os.path.getsize(pmc),
                                      avg_kb=round(os.path.getsize(pmc) / max(1, n_pmc) / 1024, 1))
        huge = gen_huge("huge", a.huge_docs, 1, 10, 1)
        doc90 = gen_huge("doc90", 1, 90, 90, 2)
        doc110 = gen_huge("doc110", 1, 110, 110, 3)
        two60 = gen_huge("two60", 2, 60, 60, 4)
        log(f"datasets: {res['datasets']}")

        log(f"provisioning {a.engine} (heap {a.heap})")
        subprocess.run([os.path.join(ROOT, "scripts/it/up.sh"), up_profile], env=env, check=True,
                       stdout=subprocess.DEVNULL)
        url = R.sh_env(os.path.join(env["IT_DIR"], "env"))["IT_ES_URL"]
        cl = R.Cluster(url)
        res["engine_version"] = cl.req("GET", "/")["version"]["number"]
        before = [i["index"] for i in cl.req("GET", "/_cat/indices?format=json&expand_wildcards=all")]
        hm = json.loads(subprocess.run([edgen, "mapping", "--profile", "huge"], check=True,
                                       capture_output=True).stdout)

        def run(tag, args, want=0):
            code, wall, hwm, err = run_sampled([tool, *args])
            row = dict(run=tag, args=[x.replace(url, "$URL").replace(work, "$WORK") for x in args], exit=code,
                       want=want, wall_s=round(wall, 2), peak_rss_mb=round(hwm / 1024, 1))
            last = [x for x in err.strip().splitlines() if x.startswith("Error")]
            row["error"] = (last[-1] if last else "")[:600]
            row["retries"] = err.count("retrying in")
            if code != want:
                fail(f"{tag}: exit {code}, want {want}: {err.strip().splitlines()[-3:]}")
            log(f"{tag}: exit {code} (want {want}), {row['wall_s']} s, peak RSS {row['peak_rss_mb']} MB"
                + (f", error: {row['error'][:200]}" if code else ""))
            res["runs"].append(row)
            return row

        def verify(tag, a_, b_):
            rep = os.path.join(resdir, f"verify-{tag}.json")
            q = subprocess.run([edverify, "diff", "--slices", "1", "--a", a_, "--b", b_, "--out", rep],
                               capture_output=True, text=True)
            res["checks"][f"digest-{tag}"] = q.returncode == 0
            if q.returncode:
                fail(f"digest {tag}: {(q.stdout + q.stderr)[-300:]}")

        def fresh(index, mapping=None):
            cl.req("DELETE", f"/{index}", ok404=True)
            if mapping:
                cl.req("PUT", f"/{index}", mapping)

        def restore(tag, f, index, extra=(), want=0):
            return run(tag, ["--input", f, "--output", f"{url}/{index}", "--type=data", *extra], want)

        def dump(tag, index, extra=()):
            out = os.path.join(work, f"{tag}.json")
            run(tag, ["--input", f"{url}/{index}", "--output", out, "--type=data", *extra])
            return out

        # pmc: dynamic mapping, 1 shard
        fresh("pmc", {"settings": {"index": {"number_of_shards": 1, "number_of_replicas": 0}}})
        restore("pmc-restore", pmc, "pmc", ["--limit=500", "--concurrency=4"])
        cl.req("POST", "/pmc/_refresh")
        verify("pmc-restore", f"{url}/pmc", pmc)
        verify("pmc-dump", f"{url}/pmc", dump("pmc-dump", "pmc", ["--limit=500"]))

        # huge 1-10 MB
        fresh("huge", hm)
        restore("huge-restore", huge, "huge", ["--limit=100"])
        cl.req("POST", "/huge/_refresh")
        verify("huge-restore", f"{url}/huge", huge)
        verify("huge-dump", f"{url}/huge", dump("huge-dump", "huge", ["--limit=10"]))

        # one 90 MB document
        for tag, extra in [("doc90-maxbulk95", ["--maxBulkBytes=95mb"]), ("doc90-default", [])]:
            fresh("doc90", hm)
            restore(f"{tag}-restore", doc90, "doc90", extra)
            cl.req("POST", "/doc90/_refresh")
            verify(tag, f"{url}/doc90", doc90)
        verify("doc90-dump", f"{url}/doc90", dump("doc90-dump", "doc90"))

        # over the limit: one 110 MB document
        fresh("doc110", hm)
        r = restore("doc110-default", doc110, "doc110", want=1)
        res["checks"]["doc110-default-names-maxDocMb"] = "--maxDocMb" in r["error"]
        r = restore("doc110-maxDocMb0", doc110, "doc110", ["--maxDocMb=0", "--retryAttempts=2", "--retryDelay=1"],
                    want=1)
        res["checks"]["doc110-413-clear"] = "http.max_content_length" in r["error"]
        res["checks"]["doc110-413-not-retried"] = r["retries"] == 0
        # one 120 MB bulk of two 60 MB documents
        fresh("two60", hm)
        r = restore("two60-maxbulk200", two60, "two60", ["--maxBulkBytes=200mb", "--retryAttempts=2",
                                                         "--retryDelay=1"], want=1)
        res["checks"]["two60-413-clear"] = "http.max_content_length" in r["error"] and "--maxBulkBytes" in r["error"]
        res["checks"]["two60-413-not-retried"] = r["retries"] == 0
        fresh("two60", hm)
        restore("two60-default", two60, "two60")
        cl.req("POST", "/two60/_refresh")
        verify("two60-default", f"{url}/two60", two60)
        for k in ("doc110-default-names-maxDocMb", "doc110-413-clear", "doc110-413-not-retried",
                  "two60-413-clear", "two60-413-not-retried"):
            if not res["checks"][k]:
                fail(f"check {k}")
        res["node"] = {k: v for k, v in cl.req("GET", "/_nodes/stats/jvm")["nodes"].popitem()[1]["jvm"]["mem"].items()
                       if k in ("heap_used_in_bytes", "heap_max_in_bytes")}
        res["leak"], lf = R.leak_check(cl, work, before=before,
                                       expected={"pmc", "huge", "doc90", "doc110", "two60"})
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
        json.dump(res, open(os.path.join(resdir, "largedocs.json"), "w"), indent=1)
        if not a.keep:
            subprocess.run(["podman", "rm", "-f", f"it-es-plain-{env['RUN_ID']}"], stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL)
            subprocess.run((["rm", "-rf"] if os.geteuid() == 0 else ["podman", "unshare", "rm", "-rf"]) + [work])
    print(f"LARGEDOCS-SUMMARY engine={a.engine} {'PASS' if rc == 0 else 'FAIL'} runs={len(res['runs'])} "
          f"result=results/load/{run_id}/largedocs.json")
    for x in res["failures"]:
        print("  failure:", x)
    return rc


if __name__ == "__main__":
    sys.exit(main())
