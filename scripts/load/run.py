#!/usr/bin/env python3
"""Load runner (TST-LOAD-001..003, LOAD-012, LOAD-017).

    scripts/load/run.sh <tier> <scenario> [--place box|runner|big] [--engine es717-plain]
                        [--run-id ID] [--docs N] [--keep] [--dry-run]

The runner does the following, in order:

1. Refuses a tier/place combination that does not fit (scenarios/tiers.toml).
2. Takes the shared IT/load lock and checks free memory and disk.
3. Provisions ONE engine through scripts/it/up.sh.
4. Creates each seed index with `edgen mapping` and seeds it from `edgen gen --format bulk` through the harness's own `_bulk` client. Edge-case documents that the engine rejects are counted, not failed.
5. Takes a reference digest per index with `edverify` in cluster mode.
6. Runs the scenario steps (dump / restore / copy) with searchdump under measure.sh, and checks each against the reference with `edverify diff`.
7. Leak check: no open scroll/PIT contexts, no `.part` files, no unexpected indices.
8. Tears down. Results go to results/load/<run-id>/<scenario>.json, recording the seeds, image, tool version and per-step metrics.

Exit codes: 0 = pass, 1 = fail, 2 = refused (tier does not fit the place), 3 = skipped (resource guard; exit 1 with IT_REQUIRED=1).
"""
import argparse, base64, datetime, fcntl, glob, json, os, re, shutil, socket, ssl, subprocess, sys, threading, time, tomllib
import urllib.error, urllib.parse, urllib.request

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
HERE = os.path.dirname(os.path.abspath(__file__))
# Average uncompressed bytes per document, used for the disk estimate.
AVG_DOC = {"logs": 1000, "metrics": 400, "nested": 1500, "geo": 300, "wide": 30000,
           "huge": 3 << 20, "edge": 400}
DUMP_FACTOR = {"none": 1.0, "gzip": 0.2, "fsCompress": 0.2, "zstd": 0.15}
EXT = {"none": "", "gzip": ".gz", "fsCompress": ".gz", "zstd": ".zst"}
CFLAGS = {"none": [], "gzip": ["--compressionType=gzip"], "fsCompress": ["--fsCompress"],
          "zstd": ["--compressionType=zstd"]}
INDEX_FACTOR = 0.8  # on-disk index size / raw source size (calibrate from seeds[].store_ratio)


def log(*a):
    print("load:", *a, file=sys.stderr, flush=True)



# LOAD-017 / TST-LOAD-017 (L-811): server-side leak check asserted in the teardown of every load run.
# Index names that engines create on their own; a new hidden index outside these is reported as a
# possible staging leftover (the tool itself stages only in hidden `.name.part-*` files, never indices).
SYSTEM_INDEX_PREFIXES = (".ds-", ".tasks", ".security", ".geoip_databases", ".kibana", ".apm", ".fleet",
                         ".opendistro", ".plugins", ".opensearch", ".ql-", ".async-search", ".transform")
# Engine-created indices without a leading dot: the OpenSearch security plugin audit log and the
# query-insights plugin top_queries-YYYY.MM.DD-NNNNN export (both appear on OpenSearch profiles).
ENGINE_INDEX_PREFIXES = ("security-auditlog-", "top_queries-")


def leak_check(cl, work=None, before=None, expected=(), wait_s=15, contexts=True, allow_part=None):
    """Return (leak, failures). `cl` may be None (file-only runs).

    * `scroll_current` and `point_in_time_current` summed over `_nodes/stats/indices/search` must reach 0
      within `wait_s`. Elasticsearch has no PIT stat (pit_current None): there `open_contexts` minus the
      scrolls (`other_open_contexts`, PITs included) must reach 0 instead.
      `contexts=False` records them without failing (Node elasticdump comparisons).
    * No `.part` file under `work` (hidden staging files included; `glob` skips dotfiles, so walk).
      Cluster data dirs (`it`, `it-*` at the top of `work`) are skipped; `allow_part(rel_dir, name)`
      exempts intentional leftovers (killed runs).
    * With `before` (index names at the start): no new index outside `expected`, no new hidden index
      outside SYSTEM_INDEX_PREFIXES.
    """
    leak, fails = {}, []
    if cl is not None:
        deadline = time.time() + wait_s
        while True:
            se = [n["indices"]["search"] for n in cl.req("GET", "/_nodes/stats/indices/search")["nodes"].values()]
            scroll = sum(s.get("scroll_current", 0) for s in se)
            has_pit = any("point_in_time_current" in s for s in se)
            pit = sum(s.get("point_in_time_current", 0) for s in se) if has_pit else None
            # without the PIT stat (Elasticsearch), open_contexts counts every open reader context:
            # scrolls, PITs and searches in flight (none at teardown)
            other = max(0, sum(s.get("open_contexts", 0) for s in se) - scroll) if not has_pit else None
            if (not scroll and not pit and not other) or time.time() > deadline:
                break
            time.sleep(1)
        leak.update(scroll_current=scroll, pit_current=pit, other_open_contexts=other)
        if contexts and (scroll or pit or other):
            fails.append(f"leak: open search contexts scroll_current={scroll} point_in_time_current={pit}"
                         f" other_open_contexts={other}")
    parts = []
    if work and os.path.isdir(work):
        for dp, dns, fs in os.walk(work):
            rel = os.path.relpath(dp, work)
            if rel == ".":
                dns[:] = [d for d in dns if d != "it" and not d.startswith("it-")]
            parts += [os.path.normpath(os.path.join(rel, f)) for f in fs
                      if ".part" in f and not (allow_part and allow_part(rel, f))]
    leak.update(part_files=sorted(parts)[:20], part_files_n=len(parts))
    if parts:
        fails.append(f"leak: {len(parts)} .part file(s) left, e.g. {sorted(parts)[:3]}")
    if cl is not None and before is not None:
        after = {i["index"] for i in cl.req("GET", "/_cat/indices?format=json&expand_wildcards=all")}
        new = after - set(before) - set(expected)
        leak["unexpected_indices"] = sorted(i for i in new if not i.startswith(".") and not i.startswith(ENGINE_INDEX_PREFIXES))
        leak["new_system_indices"] = sorted(i for i in new if i.startswith(".") and i.startswith(SYSTEM_INDEX_PREFIXES))
        leak["new_hidden_indices"] = sorted(i for i in new if i.startswith(".") and not i.startswith(SYSTEM_INDEX_PREFIXES))
        if leak["unexpected_indices"] or leak["new_hidden_indices"]:
            fails.append(f"leak: indices left {leak['unexpected_indices'] + leak['new_hidden_indices']}")
    return leak, fails

def tool_path(name):
    # L2-002: LOAD_BIN_DIR runs another build (e.g. frozen 1.1.0 binaries) against the same harness
    if os.environ.get("LOAD_BIN_DIR"):
        p = os.path.join(os.environ["LOAD_BIN_DIR"], name)
        if os.path.exists(p):
            return p
        sys.exit(f"load: {name} not in LOAD_BIN_DIR={os.environ['LOAD_BIN_DIR']}")
    for d in ("release", "it", "debug"):
        p = os.path.join(ROOT, "target", d, name)
        if os.path.exists(p):
            return p
    sys.exit(f"load: {name} not built: run cargo +1.91.1 build --release --workspace --bins")


class RequestError(RuntimeError):
    """An HTTP rejection (keep its status for the seeder's bounded 429 retry)."""
    def __init__(self, method, path, status, body):
        self.status = status
        super().__init__(f"{method} {path}: HTTP {status}: {body[:500]!r}")


class Cluster:
    def __init__(self, url, user=None, pw=None, ca=None):
        self.url = url.rstrip("/")
        self.auth = None
        if user:
            self.auth = "Basic " + base64.b64encode(f"{user}:{pw}".encode()).decode()
        self.ctx = None
        if self.url.startswith("https"):
            self.ctx = ssl.create_default_context(cafile=ca) if ca else ssl._create_unverified_context()

    def req(self, method, path, body=None, ctype="application/json", ok404=False, retry_429=True):
        # Refresh/count/stats can also be rejected while the JVM recovers from
        # indexing pressure. Retry only an explicit HTTP 429, outside tool timing.
        for attempt in range(7):
            try:
                return self._req_once(method, path, body, ctype, ok404)
            except RequestError as e:
                if e.status != 429 or not retry_429 or attempt == 6:
                    raise
                delay = min(0.25 * 2 ** attempt, 4.0)
                log(f"{method} {path} throttled (429); retry {attempt + 1}/6 in {delay:g}s")
                time.sleep(delay)

    def _req_once(self, method, path, body=None, ctype="application/json", ok404=False):
        data = body if isinstance(body, (bytes, type(None))) else json.dumps(body).encode()
        r = urllib.request.Request(self.url + path, data=data, method=method)
        if data is not None:
            r.add_header("Content-Type", ctype)
        if self.auth:
            r.add_header("Authorization", self.auth)
        try:
            with urllib.request.urlopen(r, context=self.ctx, timeout=HTTP_TIMEOUT_S) as resp:
                raw = resp.read()
        except (TimeoutError, socket.timeout) as e:
            # Name the request: a bare "timed out" cannot be told apart from a seed, refresh or flush stall.
            raise TimeoutError(f"{method} {path} timed out after {HTTP_TIMEOUT_S:g}s") from e
        except urllib.error.HTTPError as e:
            if ok404 and e.code == 404:
                return None
            raise RequestError(method, path, e.code, e.read()) from None
        return json.loads(raw) if raw else None


HTTP_TIMEOUT_S = float(os.environ.get("PERF_HTTP_TIMEOUT", "300"))


def fit(tier_name, place_name, tiers, scenario, docs_override):
    """Return (ok, reason, plan) for this tier/place/scenario."""
    place = tiers["place"][place_name]
    tier = tiers["tier"].get(tier_name)
    if tier is None:
        return False, f"unknown tier {tier_name}", None
    if tier_name not in place["tiers"]:
        why = f"tier {tier_name} is not allowed on place '{place_name}' (allowed: {place['tiers']})"
        if tier.get("needs"):
            why += f"; it needs {tier['needs']}"
        return False, why, None
    # A place may cap the heap (place.shared): the run is then a scaled-down one
    # and its numbers are only comparable with baselines from the same place.
    heap_mb = min(tier["heap_mb"], place.get("heap_cap_mb", tier["heap_mb"]))
    if heap_mb > place["max_heap_mb"]:
        why = f"heap {heap_mb} MB > place max {place['max_heap_mb']} MB"
        if tier.get("needs"):
            why += f"; full size needs {tier['needs']}"
        return False, why, None
    seeds = []
    for s in scenario.get("seed", []):
        n = s["docs"]
        if n == "tier":
            n = docs_override or tier["docs"]
        elif n == "tier.edge_docs":
            n = tier.get("edge_docs", 0)
        if n:
            seeds.append(dict(s, docs=int(n)))
    allowed = tier.get("compress")
    raw = sum(s["docs"] * AVG_DOC.get(s["profile"], 1000) for s in seeds)
    dump_bytes, copies = 0.0, 1  # the source index
    # Cells clean up after themselves (targets dropped after their verify, a dump
    # kept only until the next one), so peak disk is the source plus the two
    # largest dumps and one target -- unless the scenario says keep = true.
    keep = scenario.get("keep", False)
    dumps = []
    for st in scenario.get("step", []):
        c = st.get("compress", "none")
        if st["kind"] == "dump":
            if allowed and c not in allowed:
                return False, f"tier {tier_name} only allows compression {allowed} (step uses {c})", None
            dumps.append(raw * DUMP_FACTOR[c])
        if st["kind"] in ("restore", "copy"):
            copies += 1
    dump_bytes = sum(dumps) if keep else sum(sorted(dumps)[-2:])
    if not keep:
        copies = min(copies, 2)
    need_mb = int((raw * INDEX_FACTOR * copies + dump_bytes) / 1e6) + 500
    if need_mb > place["max_disk_mb"]:
        return False, f"estimated disk {need_mb} MB > place budget {place['max_disk_mb']} MB", None
    return True, "", dict(seeds=seeds, need_disk_mb=need_mb, heap_mb=heap_mb, place=place,
                          scaled=heap_mb < tier["heap_mb"])


def meminfo_avail_mb():
    for line in open("/proc/meminfo"):
        if line.startswith("MemAvailable:"):
            return int(line.split()[1]) // 1024
    return 0


def disk_free_mb(path):
    os.makedirs(path, exist_ok=True)
    st = os.statvfs(path)
    return st.f_bavail * st.f_frsize // (1 << 20)


def sh_env(path):
    """Read the exports of up.sh's env file without echoing them."""
    out = subprocess.run(["bash", "-c", 'set -a; source "$1"; env -0', "_", path],
                         check=True, capture_output=True).stdout
    return dict(kv.split("=", 1) for kv in out.decode().split("\0") if "=" in kv)


def engine_mapping_flavour(engine):
    # es710-plain is the OSS build (bitnamilegacy 7.10.2): no X-Pack `flattened`, so use the
    # plain disabled-object variant (edgen `os1`), like OpenSearch 1.x.
    if engine == "es710-plain":
        return "os1"
    if engine.startswith("os13") or engine.startswith("opensearch1"):
        return "os1"
    if engine.startswith("os") or engine.startswith("opensearch"):
        return "os"
    return "es"


def seed_bulk(cl, batch):
    """One synchronous bulk at a time. Retry only rejected items, never successes.

    A request-level 400 rejects the whole bulk (e.g. an oversized edge-case ID);
    isolate it pair by pair. Request/item 429s are pressure, not invalid documents:
    retry with capped exponential backoff, and fail after seven attempts.
    """
    pending, finished = batch, []
    for attempt in range(7):
        try:
            reply = cl.req("POST", "/_bulk", b"".join(pending), "application/x-ndjson", retry_429=False)
        except RequestError as e:
            if e.status == 400:
                if len(pending) == 2:
                    return finished + [dict(status=400, error=dict(type="request_400"))]
                for i in range(0, len(pending), 2):
                    finished.extend(seed_bulk(cl, pending[i:i + 2]))
                return finished
            if e.status != 429:
                raise
        else:
            items = reply["items"]
            if len(items) * 2 != len(pending):
                raise RuntimeError("seeding bulk response item count differs from request")
            retry = []
            for i, item in enumerate(items):
                res = next(iter(item.values()))
                if res.get("status") == 429:
                    retry.extend(pending[2 * i:2 * i + 2])
                else:
                    finished.append(res)
            pending = retry
            if not pending:
                return finished
        if attempt == 6:
            raise RuntimeError("seeding bulk still receives HTTP/item 429 after 7 attempts")
        delay = min(0.25 * 2 ** attempt, 4.0)
        log(f"seed bulk throttled (429); retry {attempt + 1}/6 in {delay:g}s")
        time.sleep(delay)


def seed_batches(lines, max_docs, max_bytes):
    """Keep pairs intact; oversized docs go alone, duplicate IDs start a new bulk.

    Finishing each bulk before the next keeps duplicate-ID last-wins ordering even
    when only some items receive 429 and are retried after successful neighbours.
    """
    if max_docs <= 0 or max_bytes <= 0:
        raise ValueError("seed bulk limits must be positive")
    batch, size, ids = [], 0, set()
    it = iter(lines)
    for action in it:
        source = next(it, None)
        if source is None:
            raise RuntimeError("edgen emitted an incomplete bulk action/source pair")
        meta = next(iter(json.loads(action).values()))
        key = (meta.get("_index"), meta.get("_id"), meta.get("routing", meta.get("_routing")))
        duplicate = key[1] is not None and key in ids
        pair_size = len(action) + len(source)
        if batch and (len(batch) // 2 >= max_docs or size + pair_size > max_bytes or duplicate):
            yield batch
            batch, size, ids = [], 0, set()
        if key[1] is not None:
            ids.add(key)
        batch.extend((action, source))
        size += pair_size
        if len(batch) // 2 >= max_docs or size >= max_bytes:
            yield batch
            batch, size, ids = [], 0, set()
    if batch:
        yield batch


def seed_index(cl, edgen, s, flavour, *, max_docs=5000, max_bytes=8 << 20):
    m = json.loads(subprocess.run([edgen, "mapping", "--profile", s["profile"], "--engine", flavour],
                                  check=True, capture_output=True).stdout)
    cl.req("DELETE", "/" + s["index"], ok404=True)  # a retried seed starts from a clean index
    cl.req("PUT", "/" + s["index"], m)
    p = subprocess.Popen([edgen, "gen", "--profile", s["profile"], "--seed", str(s["seed"]),
                          "--count", str(s["docs"]), "--format", "bulk", "--index", s["index"]],
                         stdout=subprocess.PIPE, stderr=subprocess.DEVNULL)
    rejected, reasons = 0, {}
    t0 = time.time()
    try:
        for batch in seed_batches(p.stdout, max_docs, max_bytes):
            for res in seed_bulk(cl, batch):
                if res.get("status", 200) >= 300:
                    rejected += 1
                    t = (res.get("error") or {}).get("type", str(res.get("status")))
                    reasons[t] = reasons.get(t, 0) + 1
        if p.wait() != 0:
            raise RuntimeError("edgen gen failed")
    finally:
        p.stdout.close()
        if p.poll() is None:
            p.terminate()
        p.wait()
    cl.req("POST", f"/{s['index']}/_refresh")
    count = cl.req("GET", f"/{s['index']}/_count")["count"]
    cl.req("POST", f"/{s['index']}/_flush")
    store = cl.req("GET", f"/{s['index']}/_stats/store")["_all"]["primaries"]["store"]["size_in_bytes"]
    return dict(index=s["index"], profile=s["profile"], seed=s["seed"], generated=s["docs"],
                stored=count, rejected=rejected, store_bytes=store,
                # same-_id edge cases overwrite each other (not an error)
                overwritten=s["docs"] - count - rejected,
                store_ratio=round(store / max(1, s["docs"] * AVG_DOC.get(s["profile"], 1000)), 3), reject_reasons=reasons,
                seconds=round(time.time() - t0, 2), mapping_engine=flavour,
                bulk_max_docs=max_docs, bulk_max_bytes=max_bytes)


def cpu_list(spec):
    """'0-3,6' -> {0, 1, 2, 3, 6}"""
    out = set()
    for part in str(spec).split(","):
        lo, _, hi = part.partition("-")
        out.update(range(int(lo), int(hi or lo) + 1))
    return out


def raw_member(line, key):
    """Raw text of a top-level member of a one-line JSON object (no re-serialisation),
    so a reference built from it is byte-identical to what the tool wrote."""
    depth, in_str, esc, start, last, cur, vstart = 0, False, False, None, None, None, None
    for i, ch in enumerate(line):
        if in_str:
            if esc:
                esc = False
            elif ch == "\\":
                esc = True
            elif ch == '"':
                in_str = False
                if depth == 1 and vstart is None:
                    last = line[start + 1:i]
            continue
        if ch == '"':
            in_str, start = True, i
        elif ch in "{[":
            depth += 1
        elif ch in "}]" or (ch == "," and depth == 1):
            if ch != ",":
                depth -= 1
            if depth <= 1 and vstart is not None and (ch == "," or depth == 0):
                return line[vstart:i].strip()
            if depth == 1 and vstart is not None and ch in "}]":
                return line[vstart:i + 1].strip()
        elif ch == ":" and depth == 1:
            cur = last
            if cur == key:
                vstart = i + 1
    return None


class NodeWatch(threading.Thread):
    """Samples the RSS of this run's engine container (its process tree, read from
    the host's /proc) every 2 s. Rootless podman with the cgroupfs manager accepts
    --memory/--cpus but does not enforce them on this box, so a place can set
    node_rss_mb: above it the container is killed and the run fails."""

    def __init__(self, rid, cap_mb):
        super().__init__(daemon=True)
        self.rid, self.cap_mb, self.peak_mb, self.breach = rid, cap_mb, 0, None
        self.stop = threading.Event()

    def tree_rss_mb(self, root):
        kids = {}
        for d in os.listdir("/proc"):
            if d.isdigit():
                try:
                    st = open(f"/proc/{d}/stat").read()
                    kids.setdefault(int(st.rsplit(")", 1)[1].split()[1]), []).append(int(d))
                except OSError:
                    pass
        total, todo = 0, [root]
        while todo:
            pid = todo.pop()
            todo += kids.get(pid, [])
            try:
                for line in open(f"/proc/{pid}/status"):
                    if line.startswith("VmRSS:"):
                        total += int(line.split()[1])
            except OSError:
                pass
        return total // 1024

    def run(self):
        while not self.stop.wait(2):
            ids = subprocess.run(["podman", "ps", "-q", "--filter", "label=searchdump-it=1",
                                  "--filter", f"name={self.rid}"], capture_output=True, text=True).stdout.split()
            for cid in ids:
                pid = subprocess.run(["podman", "inspect", "--format", "{{.State.Pid}}", cid],
                                     capture_output=True, text=True).stdout.strip()
                if not pid.isdigit() or pid == "0":
                    continue
                mb = self.tree_rss_mb(int(pid))
                self.peak_mb = max(self.peak_mb, mb)
                if self.cap_mb and mb > self.cap_mb and not self.breach:
                    self.breach = f"engine RSS {mb} MB > node_rss_mb {self.cap_mb} MB: container killed"
                    log(self.breach)
                    subprocess.run(["podman", "kill", cid], capture_output=True)


def open_text(path):
    if path.endswith(".gz"):
        import gzip
        return gzip.open(path, "rt", encoding="utf-8")
    if path.endswith(".zst"):
        raise RuntimeError("verify = 'source' needs an uncompressed (or gzip) full dump before it")
    return open(path, encoding="utf-8")


def positive_int(value):
    n = int(value)
    if n < 1:
        raise argparse.ArgumentTypeError("must be a positive integer")
    return n


def verification_limits(tier, size=None, slices=None):
    # Only the small 1 GB smoke changes. Larger tiers keep edverify's defaults.
    return dict(size=size if size is not None else (100 if tier == "L0" else 1000),
                slices=slices if slices is not None else (1 if tier == "L0" else 4))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("tier")
    ap.add_argument("scenario")
    ap.add_argument("--place", default=os.environ.get("LOAD_PLACE", "box"))
    ap.add_argument("--engine", default=None)
    ap.add_argument("--run-id", default=None)
    ap.add_argument("--docs", type=int, default=None, help="override the tier's doc count (smoke runs)")
    ap.add_argument("--verify-size", type=positive_int, help="edverify page size (L0: 100, other tiers: 1000)")
    ap.add_argument("--verify-slices", type=positive_int, help="edverify scroll slices (L0: 1, other tiers: 4)")
    ap.add_argument("--keep", action="store_true", help="keep the cluster and work dir")
    ap.add_argument("--dry-run", action="store_true", help="only print the fit decision")
    a = ap.parse_args()
    verification = verification_limits(a.tier, a.verify_size, a.verify_slices)

    tiers = tomllib.load(open(os.path.join(HERE, "scenarios", "tiers.toml"), "rb"))
    spath = a.scenario if os.path.exists(a.scenario) else os.path.join(HERE, "scenarios", a.scenario + ".toml")
    scenario = tomllib.load(open(spath, "rb"))
    engine = a.engine or scenario.get("engine", "es717-plain")
    if a.place not in tiers["place"]:
        sys.exit(f"load: unknown place {a.place}")
    ok, why, plan = fit(a.tier, a.place, tiers, scenario, a.docs)
    if not ok:
        print(f"LOAD-SUMMARY scenario={scenario['name']} tier={a.tier} place={a.place} REFUSED: {why}")
        return 2
    log(f"fit: tier={a.tier} place={a.place} engine={engine} est_disk={plan['need_disk_mb']}MB heap={plan['heap_mb']}MB")
    if a.dry_run:
        print(json.dumps(dict(scenario=scenario["name"], tier=a.tier, place=a.place, engine=engine,
                              seeds=plan["seeds"], need_disk_mb=plan["need_disk_mb"], heap_mb=plan["heap_mb"],
                              scaled=plan["scaled"], verification=verification), indent=1))
        return 0

    # Guards: lock, memory and disk (TST-INF-007).
    lock = open("/tmp/searchdump-it.lock", "w")
    if os.environ.get("IT_LOCK_HELD") != "1":
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            log("waiting for the IT/load lock (/tmp/searchdump-it.lock)")
            fcntl.flock(lock, fcntl.LOCK_EX)
    run_id = a.run_id or datetime.datetime.now().strftime("%Y%m%dT%H%M%S") + f"-{a.tier}"
    work = os.path.join(os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"), run_id)
    need_mem = plan["heap_mb"] * 2 + 512 + plan["place"]["tool_mem_mb"] + 256
    skip = None
    if meminfo_avail_mb() < need_mem:
        skip = f"MemAvailable {meminfo_avail_mb()} MB < {need_mem} MB"
    elif disk_free_mb(os.path.dirname(work)) < plan["need_disk_mb"]:
        skip = f"free disk {disk_free_mb(os.path.dirname(work))} MB < {plan['need_disk_mb']} MB"
    if skip:
        print(f"LOAD-SUMMARY scenario={scenario['name']} tier={a.tier} SKIPPED (resource guard: {skip})")
        return 1 if os.environ.get("IT_REQUIRED") == "1" else 3

    edgen, edverify, tool = tool_path("edgen"), tool_path("edverify"), tool_path("searchdump")
    os.makedirs(work, mode=0o700, exist_ok=True)
    place = plan["place"]
    # place.cpus: CPU affinity for this process and everything it starts (the engine
    # container inherits it through podman), the one CPU cap that holds on this box.
    if place.get("cpus") is not None:
        os.sched_setaffinity(0, cpu_list(place["cpus"]))
    rid = f"load{os.getpid()}"
    # place.data_on_work: the engine's data dir lives under the work dir (LOAD_WORK),
    # not in podman's container storage, and goes with it.
    if place.get("data_on_work"):
        it_dir = os.path.join(work, "it")
    else:
        it_dir = os.path.join(os.environ.get("XDG_RUNTIME_DIR") or os.path.expanduser("~/.cache"), "searchdump-load")
    env = dict(os.environ, IT_DIR=it_dir, IT_LOCK_HELD="1", RUN_ID=rid)
    if place.get("data_on_work"):
        env["IT_ES_DATA_MOUNT"] = "1"
    if place.get("container_cpus"):
        env["IT_CONTAINER_CPUS"] = str(place["container_cpus"])
    if place.get("lean_jvm"):
        env["IT_LEAN_JVM"] = "1"
    for line in subprocess.run([os.path.join(ROOT, "scripts/it/matrix.sh"), engine],
                               check=True, capture_output=True, text=True).stdout.splitlines():
        k, v = line.split("=", 1)
        env[k] = v
    up_profile = env.pop("UP_PROFILE")
    heap = f"{plan['heap_mb']}m"
    env.update(ES_HEAP=heap, OS_HEAP=heap, IT_CONTAINER_MEM=plan["place"]["container_mem"])

    result = dict(schema="edrs-load/v1", run_id=run_id, scenario=scenario["name"], tier=a.tier,
                  place=a.place, engine=engine, up_profile=up_profile,
                  image=env.get("ES_IMAGE") or env.get("OS_IMAGE"), heap=heap, scaled=plan["scaled"],
                  cpus=sorted(os.sched_getaffinity(0)), verification=verification,
                  git=subprocess.run(["git", "-C", ROOT, "rev-parse", "--short", "HEAD"],
                                     capture_output=True, text=True).stdout.strip(),
                  tool=subprocess.run([tool, "--version"], capture_output=True, text=True).stdout.strip(),
                  started=datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
                  seeds=[], steps=[], leak={}, failures=[])
    resdir = os.path.join(ROOT, "results", "load", run_id)
    os.makedirs(resdir, exist_ok=True)
    rc = 1
    watch = NodeWatch(rid, place.get("node_rss_mb"))
    try:
        watch.start()
        log(f"provisioning {engine} ({up_profile}, heap {heap})")
        subprocess.run([os.path.join(ROOT, "scripts/it/up.sh"), up_profile], env=env, check=True,
                       stdout=subprocess.DEVNULL)
        ce = sh_env(os.path.join(it_dir, "env"))
        url, user, pw, ca = ce["IT_ES_URL"], ce.get("IT_ADMIN_USER"), ce.get("IT_ADMIN_PW"), ce.get("IT_CA")
        cl = Cluster(url, user, pw, ca)
        result["engine_version"] = cl.req("GET", "/")["version"]["number"]
        # Credentials reach the tools through env only, never argv (measure.py records argv).
        tenv = dict(os.environ)
        if user:
            tenv.update(EDV_USER=user, EDV_PASSWORD=pw,
                        SEARCHDUMP_INPUT_USERNAME=user, SEARCHDUMP_INPUT_PASSWORD=pw,
                        SEARCHDUMP_OUTPUT_USERNAME=user, SEARCHDUMP_OUTPUT_PASSWORD=pw)
        tls_tool = ["--ca", ca] if ca and url.startswith("https") else []
        tls_edv = ["--ca", ca] if ca and url.startswith("https") else []
        flavour = engine_mapping_flavour(engine)
        before = {i["index"] for i in cl.req("GET", "/_cat/indices?format=json&expand_wildcards=all")}

        for s in plan["seeds"]:
            log(f"seeding {s['index']}: {s['docs']} {s['profile']} docs (seed {s['seed']})")
            # L0 uses a 1 GB heap. Smaller synchronous bulks avoid large transient
            # allocations during seed (outside the measured dump/restore steps).
            limits = dict(max_docs=500, max_bytes=1 << 20) if a.tier == "L0" else {}
            result["seeds"].append(seed_index(cl, edgen, s, flavour, **limits))
        expected = {s["index"] for s in plan["seeds"]}

        def verify(name, a_src, b_src, extra=()):
            rep = os.path.join(resdir, f"verify-{name}.json")
            b_src = [b_src] if isinstance(b_src, str) else list(b_src)
            p = subprocess.run([edverify, "diff", "--a", a_src, "--b", *b_src, "--index-as", "x",
                                "--out", rep, "--size", str(verification["size"]),
                                "--slices", str(verification["slices"]),
                                *tls_edv, *extra], env=tenv, capture_output=True, text=True)
            v = json.load(open(rep)) if os.path.exists(rep) else {}
            ok = p.returncode == 0
            if not ok:
                result["failures"].append(f"verify {name}: {p.stdout.strip()[-2000:]} {p.stderr.strip()[-400:]}")
            return dict(name=name, ok=ok, report=os.path.relpath(rep, ROOT),
                        a_count=v.get("a", {}).get("count"), b_count=v.get("b", {}).get("count"))

        def run_tool(name, args, docs, outputs=()):
            mout = os.path.join(resdir, f"metrics-{name}.json")
            cmd = [os.path.join(HERE, "measure.sh"), "--run-id", f"{run_id}-{name}", "--out", mout,
                   "--cluster", url, "--docs", str(docs), "--label", f"step={name}",
                   "--label", f"tier={a.tier}", "--label", f"engine={engine}"]
            for o in outputs:
                cmd += ["--output", o]
            p = subprocess.run(cmd + ["--", tool, *args, *tls_tool], env=tenv, cwd=work)
            m = json.load(open(mout)) if os.path.exists(mout) else {}
            val = subprocess.run([os.path.join(HERE, "measure.py"), "--validate", mout], capture_output=True, text=True)
            if val.returncode != 0:  # LOAD-003: every field present, schema-valid
                result["failures"].append(f"{name}: metrics schema: {val.stdout.strip()[-400:]}")
            if p.returncode != 0:
                result["failures"].append(f"{name}: exit {p.returncode}")
            if m.get("failed_writes"):
                result["failures"].append(f"{name}: failed_writes={m['failed_writes']}")
            if m.get("total_writes") is not None and m["total_writes"] != docs:
                result["failures"].append(f"{name}: total_writes={m['total_writes']} != {docs}")
            rss = m.get("max_rss_kb") or 0
            if rss > plan["place"]["tool_mem_mb"] * 1024:
                result["failures"].append(f"{name}: max RSS {rss} KB > {plan['place']['tool_mem_mb']} MB budget")
            return dict(name=name, exit=p.returncode, metrics=os.path.relpath(mout, ROOT),
                        wall_s=m.get("wall_s"), max_rss_kb=rss, docs_per_s=m.get("docs_per_s"),
                        retries=m.get("retries"))

        keep = a.keep or scenario.get("keep", False)

        def drop(files):
            for f in files or ():
                if not keep and os.path.exists(f):
                    os.remove(f)

        def check_node():
            if watch.breach:
                raise RuntimeError(watch.breach)

        for s in result["seeds"]:
            idx, src = s["index"], f"{url}/{s['index']}"
            dumped = None  # files of the last cluster-verified dump (input for restore / sourceOnly)
            for st in scenario.get("step", []):
                check_node()
                kind, extra = st["kind"], list(st.get("args", []))
                name = f"{st.get('name', kind)}-{idx}"
                if kind == "dump":
                    c, mode, split = st.get("compress", "none"), st.get("verify", "cluster"), st.get("split", False)
                    base = os.path.join(work, name + ".ndjson")
                    f = base if split else base + EXT[c]
                    cflags = CFLAGS[c] + ([f"--compressionLevel={st['level']}"] if "level" in st else [])
                    docs, ref, ref_idx, ref_file = s["stored"], src, None, None
                    if mode == "query":
                        # Correctness gate for a filtered dump: the engine's own _reindex of
                        # the same query is the reference set.
                        ref_idx = f"{idx}-ref-{st.get('name', 'query')}"
                        cl.req("POST", "/_reindex?refresh=true&wait_for_completion=true",
                               {"source": {"index": idx, **st["query"]}, "dest": {"index": ref_idx}})
                        docs = cl.req("GET", f"/{ref_idx}/_count")["count"]
                        ref = f"{url}/{ref_idx}"
                        extra.append("--searchBody=" + json.dumps(st["query"], separators=(",", ":")))
                    elif mode == "source":
                        # --sourceOnly lines carry no _index/_id: the reference is the raw
                        # _source of every line of the last cluster-verified dump.
                        if not dumped:
                            raise RuntimeError(f"{name}: verify = 'source' needs an earlier full dump step")
                        ref_file = os.path.join(work, name + ".ref.ndjson")
                        with open(ref_file, "w", encoding="utf-8") as out:
                            for df in dumped:
                                with open_text(df) as fin:
                                    for line in fin:
                                        if line.strip():
                                            out.write(raw_member(line.strip(), "_source") + "\n")
                        ref = ref_file
                        if "--sourceOnly" not in extra:
                            extra.append("--sourceOnly")
                    elif mode != "cluster":
                        raise RuntimeError(f"{name}: unknown verify mode {mode}")
                    r = run_tool(name, ["--input", src, "--output", f, "--type=data", *cflags, *extra],
                                 docs, [] if split else [f])
                    if split:
                        pat = re.compile(re.escape(name) + r"\.split-(\d+)\.ndjson")
                        files = sorted((p for p in glob.glob(os.path.join(work, name + ".split-*"))
                                        if pat.match(os.path.basename(p))),
                                       key=lambda p: int(pat.match(os.path.basename(p)).group(1)))
                        r["parts"] = len(files)
                        r["bytes_out"] = sum(os.path.getsize(p) for p in files)
                        if len(files) < st.get("min_parts", 2):
                            result["failures"].append(f"{name}: {len(files)} part(s) < min_parts {st.get('min_parts', 2)}")
                    else:
                        files = [f]
                    r["verify"] = verify(name, ref, files)
                    if ref_idx:
                        r["reference_docs"] = docs
                        cl.req("DELETE", "/" + ref_idx, ok404=True)
                    if mode == "cluster":
                        if dumped:
                            drop(dumped)
                        dumped = files
                    else:
                        drop(files + ([ref_file] if ref_file else []))
                elif kind in ("restore", "copy"):
                    dst = f"{idx}-{st.get('name', kind)}"
                    m = json.loads(subprocess.run([edgen, "mapping", "--profile", s["profile"], "--engine", flavour],
                                                  check=True, capture_output=True).stdout)
                    cl.req("PUT", "/" + dst, m)
                    if kind == "restore":
                        if not dumped:
                            raise RuntimeError("restore step needs an earlier dump step")
                        if len(dumped) != 1:
                            raise RuntimeError("restore step needs an unsplit dump")
                        inp = dumped[0]
                    else:
                        inp = src
                    r = run_tool(name, ["--input", inp, "--output", f"{url}/{dst}", "--type=data", *extra],
                                 s["stored"])
                    cl.req("POST", f"/{dst}/_refresh")
                    r["verify"] = verify(name, src, f"{url}/{dst}")
                    if keep:
                        expected.add(dst)
                    else:  # one target at a time: keeps the disk peak to source + one copy
                        cl.req("DELETE", "/" + dst, ok404=True)
                else:
                    raise RuntimeError(f"unknown step kind {kind}")
                r["cell"] = st.get("name", kind)
                result["steps"].append(r)
            drop(dumped)
        check_node()

        # LOAD-017 leak check
        result["leak"], f = leak_check(cl, work, before=before, expected=expected)
        result["failures"] += f
        rc = 0 if not result["failures"] else 1
    except Exception as e:  # report, then tear down
        result["failures"].append(f"error: {e}")
        rc = 1
    finally:
        watch.stop.set()
        result["node"] = dict(peak_rss_mb=watch.peak_mb, node_rss_mb=place.get("node_rss_mb"),
                              container_cpus=env.get("IT_CONTAINER_CPUS"), breach=watch.breach)
        result["finished"] = datetime.datetime.now().astimezone().isoformat(timespec="seconds")
        result["pass"] = rc == 0
        out = os.path.join(resdir, scenario["name"] + ".json")
        with open(out, "w") as f:
            json.dump(result, f, indent=1)
        if not a.keep:
            subprocess.run([os.path.join(ROOT, "scripts/it/down.sh")], env=env)
            shutil.rmtree(work, ignore_errors=True)
            shutil.rmtree(it_dir, ignore_errors=True)
        else:
            log(f"kept: work={work} cluster env={it_dir}/env")
    steps = " ".join(f"{s['name']}={'ok' if s.get('verify', {}).get('ok') and not s['exit'] else 'FAIL'}"
                     f"({s['docs_per_s']}d/s,{s['max_rss_kb']}KB)" for s in result["steps"])
    print(f"LOAD-SUMMARY scenario={scenario['name']} tier={a.tier} engine={engine} "
          f"{'PASS' if rc == 0 else 'FAIL'} {steps} result={os.path.relpath(out, ROOT)}")
    for f in result["failures"]:
        print("  failure:", f)
    return rc


if __name__ == "__main__":
    sys.exit(main())
