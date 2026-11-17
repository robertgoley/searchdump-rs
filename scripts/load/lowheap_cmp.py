#!/usr/bin/env python3
"""Compare lowheap.json runs side by side: wall (s), exit, 429 lines per dump/load step, plus the
bulk-split and page-halving events found in the kept child logs (--keep work dirs).

    python3 scripts/load/lowheap_cmp.py NAME=results/load/<run>/lowheap.json [NAME=...] [--work /var/tmp/searchdump-load]
"""
import glob, json, os, re, sys

def events(work, run_id, eng, heap, step):
    """(splits, halvings, smallest split cap KB, causes) from the kept child logs of one step."""
    logs = glob.glob(os.path.join(work, run_id, f"{eng}-{heap}", f"tmp-{step}", "*", "*.data.log"))
    splits = halv = 0
    cap = None
    causes = set()
    for f in logs:
        for line in open(f, errors="replace"):
            m = re.search(r"sending later bulks at <= (\d+) KB", line)
            if m:
                splits += 1
                cap = min(cap or 10**9, int(m.group(1)))
            if "halving the page" in line:
                halv += 1
            c = re.search(r"\[cause: ([a-z_]+)", line)
            if c:
                causes.add(c.group(1))
    return splits, halv, cap, causes

def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    work = "/var/tmp/searchdump-load"
    if "--work" in sys.argv:
        work = sys.argv[sys.argv.index("--work") + 1]
        args = [a for a in args if a != work]
    runs = []
    for a in args:
        name, path = a.split("=", 1)
        runs.append((name, json.load(open(path))))
    keys = []
    for _, d in runs:
        for c in d["cells"]:
            for s in c["steps"]:
                if s["kind"] in ("dump", "load") and (c["engine"], c["heap"], s["step"]) not in keys:
                    keys.append((c["engine"], c["heap"], s["step"]))
    print("| engine | heap | step | " + " | ".join(n for n, _ in runs) + " |")
    print("|---|---|---|" + "---|" * len(runs))
    for k in keys:
        cells = []
        for _, d in runs:
            s = next((s for c in d["cells"] if (c["engine"], c["heap"]) == k[:2] for s in c["steps"] if s["step"] == k[2]), None)
            if not s:
                cells.append("—"); continue
            sp, hv, cap, causes = events(work, d["run_id"], *k)
            t = f"{s['wall_s']:.1f}" if s["exit"] == 0 else f"**exit {s['exit']}** {s['wall_s']:.1f}"
            ex = []
            if s.get("retries"): ex.append(f"retry×{s['retries']}")
            if sp: ex.append(f"split×{sp}→{cap} KB")
            if hv: ex.append(f"halve×{hv}")
            if s.get("http429") and not (sp or hv): ex.append(f"429×{s['http429']}")
            if s.get("circuit_breaking"): ex.append(f"cb×{s['circuit_breaking']}")
            if causes: ex.append("/".join(sorted(causes)))
            cells.append(t + (" (" + ", ".join(ex) + ")" if ex else ""))
        print(f"| {k[0]} | {k[1]} | {k[2]} | " + " | ".join(cells) + " |")

if __name__ == "__main__":
    main()
