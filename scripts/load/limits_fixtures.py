#!/usr/bin/env python3
"""1.2-1d (L2-004): capture real `_nodes/stats` / `_nodes/settings` / `_cat/thread_pool` bodies
for the `--serverLimits` unit tests (tests/fixtures/server-limits/).

    scripts/load/limits_fixtures.py [--engines es710,es861,es819,es912,os13,os2,os3]
                                    [--heaps 256m,384m,1024m] [--port 19415]

Each engine x heap is started with scripts/it/up.sh (plain profile, one node), the three probe
requests searchdump makes are sent with the same filter_path, and the answers are written as
<engine>-<heap>.nodes-stats.json, .nodes-settings.json, .thread-pool.json. Node ids, names and
addresses are replaced with stable placeholders (no secrets are involved: plain profiles). An engine
that does not boot at a heap is recorded in boot.json (the OpenSearch small-heap floor, section 9
Q7).
"""
import argparse, json, os, subprocess, sys, time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import run as R  # noqa: E402

ROOT = R.ROOT
ENGINES = {"es710": "es710-plain", "es861": "elasticsearch8-plain", "es819": "es819-plain",
           "es912": "elasticsearch-plain", "os13": "os13-plain", "os2": "opensearch-plain",
           "os3": "opensearch-plain"}
FILTER = ("nodes.*.name,nodes.*.roles,nodes.*.jvm.mem.heap_max_in_bytes,nodes.*.jvm.mem.heap_used_percent,"
          "nodes.*.breakers.parent,nodes.*.breakers.request,nodes.*.indexing_pressure.memory.limit_in_bytes")


def scrub(v, names):
    if isinstance(v, dict) and "nodes" in v and isinstance(v["nodes"], dict):
        out = {}
        for i, (nid, n) in enumerate(sorted(v["nodes"].items())):
            n = dict(n)
            if "name" in n:
                names[n["name"]] = f"node-{i}"
                n["name"] = f"node-{i}"
            out[f"node-id-{i}"] = n
        return dict(v, nodes=out)
    if isinstance(v, list):
        return [dict(r, node_name=names.get(r.get("node_name"), r.get("node_name"))) for r in v]
    return v


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engines", default=",".join(ENGINES))
    ap.add_argument("--heaps", default="256m,384m,1024m")
    ap.add_argument("--port", type=int, default=19415)
    ap.add_argument("--out", default=os.path.join(ROOT, "tests/fixtures/server-limits"))
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    work = os.path.join(os.environ.get("LOAD_WORK", "/var/tmp/searchdump-load"), f"limfix-{os.getpid()}")
    os.makedirs(work, mode=0o700, exist_ok=True)
    boot = {}
    bp = os.path.join(a.out, "boot.json")
    if os.path.exists(bp):
        boot = json.load(open(bp))
    for e in [x for x in a.engines.split(",") if x]:
        for h in [x for x in a.heaps.split(",") if x]:
            base = {}
            for line in subprocess.run([os.path.join(ROOT, "scripts/it/matrix.sh"), ENGINES[e]], check=True,
                                       capture_output=True, text=True).stdout.splitlines():
                k, v = line.split("=", 1)
                base[k] = v
            profile = base.pop("UP_PROFILE")
            if e == "os2":
                base.update(OS_TAG="2", OS_IMAGE="docker.io/opensearchproject/opensearch:2")
            if e == "os3":
                base.update(OS_TAG="3", OS_IMAGE="docker.io/opensearchproject/opensearch:3")
            mb = int(h.rstrip("m"))
            run_id = f"lf{os.getpid()}{e}{mb}"
            env = {k: v for k, v in os.environ.items()
                   if not k.startswith(("IT_", "ES_", "OS_")) and k not in ("RUN_ID", "PORT", "UP_PROFILE")}
            env.update(base, IT_DIR=os.path.join(work, f"it-{e}-{mb}"), IT_LOCK_HELD="1", RUN_ID=run_id,
                       ES_HEAP=h, OS_HEAP=h, PORT=str(a.port), IT_CONTAINER_MEM=f"{mb * 2 + 1024}m")
            key = f"{e}-{h}"
            t0 = time.time()
            try:
                subprocess.run([os.path.join(ROOT, "scripts/it/up.sh"), profile], env=env, check=True,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=600)
                url = R.sh_env(os.path.join(env["IT_DIR"], "env"))["IT_ES_URL"]
                cl = R.Cluster(url)
                info = cl.req("GET", "/")
                ver = info["version"]["number"]
                dist = info["version"].get("distribution", "elasticsearch")
                metrics = "jvm,breaker" if (dist == "elasticsearch" and tuple(map(int, ver.split(".")[:2])) < (7, 9)) \
                    else "jvm,breaker,indexing_pressure"
                names = {}
                stats = scrub(cl.req("GET", f"/_nodes/stats/{metrics}?filter_path={FILTER}"), names)
                settings = scrub(cl.req("GET", "/_nodes/settings?filter_path=nodes.*.name,"
                                        "nodes.*.settings.http.max_content_length"), names)
                pools = scrub(cl.req("GET", "/_cat/thread_pool/search,write?h=node_name,name,size&format=json"), names)
                for suffix, body in (("nodes-stats", stats), ("nodes-settings", settings), ("thread-pool", pools)):
                    with open(os.path.join(a.out, f"{key}.{suffix}.json"), "w") as fo:
                        json.dump(body, fo, indent=1, sort_keys=True)
                        fo.write("\n")
                boot[key] = dict(booted=True, version=f"{dist} {ver}", seconds=round(time.time() - t0, 1))
                print(f"{key}: {dist} {ver} captured", flush=True)
            except Exception as ex:
                boot[key] = dict(booted=False, error=repr(ex)[:200], seconds=round(time.time() - t0, 1))
                print(f"{key}: did not boot / capture: {ex!r}"[:300], flush=True)
            finally:
                subprocess.run([os.path.join(ROOT, "scripts/it/down.sh")], env=env,
                               stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            with open(bp, "w") as fo:
                json.dump(boot, fo, indent=1, sort_keys=True)
                fo.write("\n")
    subprocess.run(["podman", "unshare", "rm", "-rf", work])
    return 0


if __name__ == "__main__":
    sys.exit(main())
