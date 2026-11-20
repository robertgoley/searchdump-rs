#!/usr/bin/env python3
"""Supplementary report for a Node / 1.0 / 1.1 / 1.2 release comparison directory (relcmp layout,
e.g. results/local/relcmp-20261117): binary SHA-256 header, headline group table, 1.2-vs-1.1
outliers, low-memory timing tables and every Suite C row (including the 1-core / 4-core pinned
rows). Reads only the committed JSON/TSV files; writes Markdown to stdout.

    python3 scripts/perf/relcmp_report.py results/local/relcmp-20261117 > .../REPORT-EXTRA.md
"""
import csv, json, os, statistics as st, sys

PASSES = (("1.0.0", "10"), ("1.1.0", "11"), ("1.2.0", "12"))
GROUPS = [("dump", lambda c: c.startswith("dump-") and not c.startswith(("dump-scroll", "dump-pit", "dump-split"))),
          ("dump sliced", lambda c: c.startswith(("dump-scroll", "dump-pit"))),
          ("dump split", lambda c: c.startswith("dump-split")),
          ("restore", lambda c: c.startswith("restore-")),
          ("copy", lambda c: c.startswith("copy")),
          ("mapping/settings", lambda c: c.startswith(("mapping-", "settings-"))),
          ("multi dump", lambda c: c.startswith("multi-dump")),
          ("multi restore", lambda c: c.startswith("multi-restore"))]
MIN_NODE_WALL = 0.5


def group_of(case):
    for g, f in GROUPS:
        if f(case):
            return g
    return "other"


def load_suite_a(root):
    """{engine: {(tool, case): [walls]}} with tool in node/1.0.0/1.1.0/1.2.0 (measured reps only)."""
    out, hashes = {}, {}
    base = os.path.join(root, "suite-a")
    for eng in sorted(os.listdir(base)):
        m = out.setdefault(eng, {})
        for rel, p in PASSES:
            f = os.path.join(base, eng, p, "combined.json")
            if not os.path.exists(f):
                continue
            d = json.load(open(f))
            hashes.setdefault(rel, d.get("tool_hashes", {}))
            if p == "10":
                hashes.setdefault("node", {k: v for k, v in d.get("tool_hashes", {}).items() if k.startswith("node")})
            for e in d["engines"]:
                for r in e["runs"]:
                    if r["rep"] < 0 or r["values"].get("exit_code") != 0:
                        continue
                    tool = "node" if r["tool"] == "node" else rel
                    m.setdefault((tool, r["case"]), []).append(r["values"]["wall_s"])
    return out, hashes


def med(m, tool, case):
    v = m.get((tool, case))
    return st.median(v) if v else None


def headline(A):
    rels = [r for r, _ in PASSES]
    rows = {}
    for eng, m in A.items():
        for (tool, case) in m:
            if tool != "node":
                continue
            n = med(m, "node", case)
            if n is None or n < MIN_NODE_WALL:
                continue
            for r in rels:
                t = med(m, r, case)
                if t:
                    rows.setdefault((group_of(case), r), []).append(n / t)
                    rows.setdefault(("all", r), []).append(n / t)
    print("## Headline by case group (median Node wall / tool wall, Node wall >= 0.5 s)\n")
    print("| group | cells | " + " | ".join(rels) + " |\n|---|---:|" + "---:|" * len(rels))
    for g in [g for g, _ in GROUPS] + ["all"]:
        if (g, rels[0]) in rows:
            print(f"| {g} | {len(rows[(g, rels[0])])} | " + " | ".join(f"{st.median(rows[(g, r)]):.2f}×" for r in rels) + " |")
    # Rust-only ratios (cases Node cannot run included)
    print("\n## Rust only: median wall ratio by case group (lower is faster)\n")
    print("| group | cells | 1.1/1.0 | 1.2/1.0 | 1.2/1.1 |\n|---|---:|---:|---:|---:|")
    agg = {}
    for eng, m in A.items():
        for case in {c for (_, c) in m}:
            a, b, c = (med(m, r, case) for r in rels)
            if a and b and c:
                for g in (group_of(case), "all"):
                    agg.setdefault(g, []).append((b / a, c / a, c / b))
    for g in [g for g, _ in GROUPS] + ["all"]:
        if g in agg:
            v = agg[g]
            print(f"| {g} | {len(v)} | " + " | ".join(f"{st.median(x[i] for x in v):.3f}" for i in range(3)) + " |")


def outliers(A, thr=1.25):
    print(f"\n## 1.2.0 vs 1.1.0 outliers (median ratio >= {thr} or <= {1/thr:.2f}, per engine and case)\n")
    print("| engine | case | 1.1.0 s | 1.2.0 s | 1.2/1.1 | 1.2.0 per-rep walls |\n|---|---|---:|---:|---:|---|")
    for eng, m in A.items():
        for case in sorted({c for (_, c) in m}):
            a, b = med(m, "1.1.0", case), med(m, "1.2.0", case)
            if a and b and (b / a >= thr or b / a <= 1 / thr) and max(a, b) >= 0.05:
                print(f"| {eng} | {case} | {a:.3f} | {b:.3f} | {b/a:.2f} | {', '.join(f'{x:.3f}' for x in m[('1.2.0', case)])} |")


def lowmem(root):
    rels = ["node", "1.0.0", "1.1.0", "1.2.0"]
    D = {}
    for r in rels:
        f = os.path.join(root, "lowheap", f"{r}.json")
        if os.path.exists(f):
            D[r] = json.load(open(f))
    if not D:
        return
    print("\n## Low memory: dump/load wall (s), exit, HTTP 429 lines, breaker trips\n")
    print("`429` counts log lines containing \"429\" (1.2 split/halve warnings included); `retries` in "
          "lowheap.json counts only \"retrying in\" lines, so a 1.2 split shows 429>0 with retries=0.\n")
    print("| engine | heap | step | " + " | ".join(D) + " |\n|---|---|---|" + "---|" * len(D))
    keys = []
    for r, d in D.items():
        for c in d["cells"]:
            for s in c["steps"]:
                if (s.get("kind") or s["step"].split("-")[0]) in ("dump", "load"):
                    k = (c["engine"], c["heap"], s["step"])
                    if k not in keys:
                        keys.append(k)
    idx = {r: {(c["engine"], c["heap"], s["step"]): s for c in d["cells"] for s in c["steps"]} for r, d in D.items()}
    for k in keys:
        cells = []
        for r in D:
            s = idx[r].get(k)
            if not s:
                cells.append("—")
                continue
            t = f"{s['wall_s']:.1f}" if s.get("exit") == 0 else f"**exit {s.get('exit')}** {s['wall_s']:.1f}"
            extra = []
            if s.get("http429"):
                extra.append(f"429×{s['http429']}")
            if s.get("circuit_breaking"):
                extra.append(f"cb×{s['circuit_breaking']}")
            if not s.get("node_alive", True):
                extra.append("node down")
            cells.append(t + (" (" + ", ".join(extra) + ")" if extra else ""))
        print(f"| {k[0]} | {k[1]} | {k[2]} | " + " | ".join(cells) + " |")
    if "1.2.0" in D:
        print("\n### 1.2.0 `auto` sizing per dump step (from the probe line)\n")
        print("| engine | heap | step | heap used | page budget | in flight | bulk/worker |\n|---|---|---|---:|---:|---:|---:|")
        import re
        for c in D["1.2.0"]["cells"]:
            for s in c["steps"]:
                L = s.get("limits_line") or ""
                if not L:
                    continue
                g = lambda p: (re.search(p, L) or [None, "?"])[1]
                print(f"| {c['engine']} | {c['heap']} | {s['step']} | {g(r'\((\d+) % used')}% | {g(r'page budget ([\d.]+ \w+)')} | "
                      f"{g(r'in flight ([\d.]+ \w+)')} | {g(r'bulk ([\d.]+ \w+) per')} |")


def suite_c(root):
    base = os.path.join(root, "suite-c")
    if not os.path.isdir(base):
        return
    print("\n## Suite C: every measured row (mean of runs; 2,000,000 docs)\n")
    print("| file | tool | runs | wall s | docs/s | MiB/s | CPU s | CPU/wall |\n|---|---|---:|---:|---:|---:|---:|---:|")
    order = sorted(os.listdir(base), key=lambda f: (f.split("-")[1], f))
    for f in order:
        if not f.endswith(".tsv"):
            continue
        rows = list(csv.DictReader(open(os.path.join(base, f)), delimiter="\t"))
        by = {}
        for r in rows:
            by.setdefault(r["tool"], []).append(r)
        for tool, rs in by.items():
            w = st.mean(float(r["real_seconds"]) for r in rs)
            cpu = st.mean(float(r["cpu_seconds"]) for r in rs)
            lines = st.mean(int(r["lines"]) for r in rs)
            b = st.mean(int(r["bytes"]) for r in rs)
            print(f"| {f[4:-4]} | {tool} | {len(rs)} | {w:.2f} | {lines / w:,.0f} | {b / w / 2**20:.1f} | {cpu:.2f} | {cpu / w:.2f} |")


def main():
    root = sys.argv[1]
    A, hashes = load_suite_a(root)
    print(f"# Supplementary report for `{root}`\n")
    print("Generated by `scripts/perf/relcmp_report.py` from the committed JSON/TSV only.\n")
    print("## Binary SHA-256 (from combined.json `tool_hashes`)\n")
    print("| release | binary | sha256 |\n|---|---|---|")
    for rel, h in hashes.items():
        for k in sorted(h):
            if rel == "node" or not k.startswith("node"):
                print(f"| {rel} | {k} | `{h[k]}` |")
    headline(A)
    outliers(A)
    lowmem(root)
    suite_c(root)


if __name__ == "__main__":
    main()
