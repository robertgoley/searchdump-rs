#!/usr/bin/env python3
"""Step-by-step table for a point-in-time perf run (scripts/perf/pit.sh aggregate).

  scripts/perf/pit_table.py --state /workspace/perf-pit-1.1 > pit-report.md

Reads PIT_STATE/steps.tsv (index, label, short hash, full hash, host id, subject) and the run
markers PIT_STATE/runs/<short>/<engine>.done ("rc=N dir=RESULTS harness=H"), then each run's
combined.json. Per engine: one row per case, one column per step with the searchdump wall
median and its change against the previous step; changes beyond the noise threshold are bold.
A summary gives, per step and engine, the geometric-mean wall ratio against the first step
over the cases both measured, and how many cases moved beyond noise either way.
"""
import argparse
import json
import math
import os
import sys

NOISE_MIN_PCT = 5.0  # never call a change below this significant
NOISE_CV_MULT = 2.0  # ... nor one below this many coefficients of variation


def load_steps(state):
    steps = []
    with open(os.path.join(state, "steps.tsv")) as f:
        for line in f:
            parts = line.rstrip("\n").split("\t")
            if len(parts) < 6:
                continue
            idx, label, short, full, host, subject = parts[:6]
            steps.append(dict(idx=int(idx), label=label, short=short, full=full, host=host, subject=subject))
    return steps


def load_run(state, short, engine):
    mark = os.path.join(state, "runs", short, f"{engine}.done")
    if not os.path.exists(mark):
        return None
    fields = dict(kv.split("=", 1) for kv in open(mark).read().split() if "=" in kv)
    path = os.path.join(fields.get("dir", ""), "combined.json")
    if not os.path.exists(path):
        return None
    data = json.load(open(path))
    for e in data.get("engines", []):
        if e.get("engine") == engine:
            return dict(rc=fields.get("rc"), dir=fields.get("dir"), harness=fields.get("harness"),
                        version=e.get("engine_version"), failures=e.get("failures", []),
                        cases={c["case"]: c for c in e.get("summary", [])})
    return None


def metric(case, tool="rs"):
    m = case.get(tool) if case else None
    if not m or m.get("wall_s") is None:
        return None
    return m


def pct(new, old):
    return (new - old) / old * 100.0 if new is not None and old else None


def significant(d, a, b):
    if d is None:
        return False
    cv = max((a or {}).get("wall_cv_pct") or 0.0, (b or {}).get("wall_cv_pct") or 0.0)
    return abs(d) >= max(NOISE_MIN_PCT, NOISE_CV_MULT * cv)


def fmt_cell(cur, prev):
    if cur is None:
        return "—"
    s = f"{cur['wall_s']:.3f}"
    if prev is None:
        return s
    d = pct(cur["wall_s"], prev["wall_s"])
    tag = f"{d:+.0f}%"
    return f"{s} (**{tag}**)" if significant(d, cur, prev) else f"{s} ({tag})"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--state", required=True)
    ap.add_argument("--engines", default=os.environ.get("PIT_ENGINES", "es710-plain es819-plain os3-plain"))
    a = ap.parse_args()
    steps = load_steps(a.state)
    if not steps:
        print("no steps (run `pit.sh list` first)", file=sys.stderr)
        return 2
    engines = a.engines.split()
    out = []
    w = out.append
    w("# Point-in-time perf: release/1.1, step by step\n")
    w(f"Noise threshold: a change is **bold** when it is at least {NOISE_MIN_PCT:.0f}% and at least "
      f"{NOISE_CV_MULT:.0f}× the larger wall CV of the two cells. Cells: searchdump wall median (s) "
      "and the change against the previous step.\n")
    w("| # | step | commit | perf host id | subject |")
    w("|---|---|---|---|---|")
    for s in steps:
        w(f"| {s['idx']} | {s['label']} | `{s['short']}` | `{s['host']}` | {s['subject']} |")
    w("")
    runs = {(s["short"], e): load_run(a.state, s["short"], e) for s in steps for e in engines}
    summary = []
    for e in engines:
        have = [s for s in steps if runs[(s["short"], e)]]
        w(f"## {e}\n")
        if not have:
            w("No finished runs yet.\n")
            continue
        harnesses = sorted({runs[(s['short'], e)]['harness'] or '?' for s in have})
        versions = sorted({str(runs[(s['short'], e)]['version']) for s in have})
        w(f"Engine version {', '.join(versions)}; harness {', '.join(harnesses)}; "
          f"{len(have)}/{len(steps)} steps measured.\n")
        for s in have:
            r = runs[(s["short"], e)]
            if r["failures"] or r["rc"] not in ("0", None):
                w(f"- step {s['label']} `{s['short']}`: rc={r['rc']}, failures: {'; '.join(r['failures']) or 'see summary.md'}")
        case_order = []
        for s in have:
            for c in runs[(s["short"], e)]["cases"]:
                if c not in case_order:
                    case_order.append(c)
        base = runs[(have[0]["short"], e)]
        node_col = any(metric(base["cases"].get(c), "node") for c in case_order)
        head = ["case"] + ([f"Node ({have[0]['label']})"] if node_col else []) + \
               [f"{s['label']} `{s['short']}`" for s in have] + [f"last vs {have[0]['label']}"]
        w("| " + " | ".join(head) + " |")
        w("|" + "---|" * len(head))
        for c in case_order:
            row = [c]
            if node_col:
                nm = metric(base["cases"].get(c), "node")
                row.append(f"{nm['wall_s']:.3f}" if nm else "—")
            prev = None
            first = None
            last = None
            for s in have:
                cur = metric(runs[(s["short"], e)]["cases"].get(c))
                row.append(fmt_cell(cur, prev))
                if cur is not None:
                    first = first or cur
                    last = cur
                    prev = cur
            d = pct(last["wall_s"], first["wall_s"]) if first and last and first is not last else None
            row.append("—" if d is None else (f"**{d:+.0f}%**" if significant(d, first, last) else f"{d:+.0f}%"))
            w("| " + " | ".join(row) + " |")
        w("")
        for s in have[1:]:
            cur_r = runs[(s["short"], e)]
            prev_s = have[have.index(s) - 1]
            prev_r = runs[(prev_s["short"], e)]
            logs, better, worse = [], 0, 0
            for c in case_order:
                cb = metric(base["cases"].get(c))
                cc = metric(cur_r["cases"].get(c))
                cp = metric(prev_r["cases"].get(c))
                if cb and cc:
                    logs.append(math.log(cc["wall_s"] / cb["wall_s"]))
                if cp and cc:
                    d = pct(cc["wall_s"], cp["wall_s"])
                    if significant(d, cc, cp):
                        better += d < 0
                        worse += d > 0
            gm = math.exp(sum(logs) / len(logs)) if logs else None
            summary.append((s, e, gm, better, worse, len(logs)))
    if summary:
        w("## Summary per step\n")
        w(f"Geometric-mean wall ratio against step {steps[0]['label']} (below 1.00 = faster), and the "
          "cases that got faster / slower beyond noise against the previous measured step.\n")
        w("| step | commit | engine | ratio vs first | faster | slower | cases |")
        w("|---|---|---|---|---|---|---|")
        for s, e, gm, better, worse, n in summary:
            w(f"| {s['label']} | `{s['short']}` | {e} | {'—' if gm is None else f'{gm:.3f}'} | {better} | {worse} | {n} |")
        w("")
    print("\n".join(out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
