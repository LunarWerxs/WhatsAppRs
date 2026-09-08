#!/usr/bin/env python3
"""Summarise engine-bench.ps1 output: medians across runs, and the spread.

    python bench-summary.py [bench-engines.jsonl ...]

Prints the median of each measure and, beside it, the min-max range. **Read the range
before believing a difference.** On the Servo round the same build measured 8.5 fps and
31.2 fps on consecutive runs, and a "memory optimisation" derived from a single sample
turned out to halve the frame rate while saving nothing. A difference smaller than the
spread of either configuration is not a difference; this script prints both so that is
visible rather than a thing you have to remember to check.
"""

import json
import statistics
import sys
from collections import defaultdict


def med(values):
    values = [v for v in values if v is not None]
    return statistics.median(values) if values else None


def rng(values):
    values = [v for v in values if v is not None]
    if not values:
        return ""
    if len(values) == 1:
        return "single run"
    return f"{min(values):g}-{max(values):g}"


def fmt(value, width=7):
    return ("-" if value is None else f"{value:g}").rjust(width)


def main(paths):
    rows = []
    for path in paths:
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                line = line.strip()
                if line:
                    rows.append(json.loads(line))
    if not rows:
        print("no rows")
        return 1

    groups = defaultdict(list)
    for row in rows:
        key = (row["engine"], row.get("label", ""), bool(row.get("real")))
        groups[key].append(row)

    sample_points = sorted({s["at_s"] for r in rows for s in (r.get("samples") or [])})

    header = f"{'engine':<9} {'label':<22} {'acct':<6} {'n':>2}  {'ready s':>8}"
    for at in sample_points:
        header += f" {('ws@' + str(at)):>9} {('priv@' + str(at)):>10} {('proc@' + str(at)):>9} {('cpu@' + str(at)):>9}"
    header += f" {'fps':>7} {'p95 ms':>8} {'worst':>7} {'profile MB':>11}"
    print(header)
    print("-" * len(header))

    for key in sorted(groups):
        engine, label, real = key
        runs = groups[key]
        line = f"{engine:<9} {label:<22} {('real' if real else 'fresh'):<6} {len(runs):>2}"
        line += f" {fmt(med([r.get('ready_s') for r in runs]), 8)}"
        spreads = [("ready s", rng([r.get("ready_s") for r in runs]))]
        for at in sample_points:
            def pick(field):
                out = []
                for r in runs:
                    for s in r.get("samples") or []:
                        if s["at_s"] == at:
                            out.append(s.get(field))
                return out
            line += (
                f" {fmt(med(pick('ws_mb')), 9)} {fmt(med(pick('private_mb')), 10)}"
                f" {fmt(med(pick('processes')), 9)} {fmt(med(pick('cpu_s')), 9)}"
            )
            spreads.append((f"ws@{at}", rng(pick("ws_mb"))))

        def frames(field):
            out = []
            for r in runs:
                raw = r.get("frames")
                if not raw:
                    continue
                try:
                    out.append(json.loads(raw).get(field))
                except Exception:
                    pass
            return out

        line += f" {fmt(med(frames('fps')), 7)} {fmt(med(frames('p95_ms')), 8)} {fmt(med(frames('worst_ms')), 7)}"
        line += f" {fmt(med([(r.get('profile_kb') or 0) / 1024 for r in runs]), 11)}"
        print(line)
        spreads.append(("fps", rng(frames("fps"))))
        detail = "  ".join(f"{name} {value}" for name, value in spreads if value and value != "single run")
        if detail:
            print(f"{'':<9} {'':<22} spread: {detail}")
    return 0


def markdown(paths):
    """The same medians as a markdown table, for pasting into FINDINGS.md.

    Deliberately carries the spread in its own column rather than dropping it: a table in a
    document outlives the run that produced it, and a median with no spread beside it is the
    exact shape of the mistake the Servo round made.
    """
    rows = []
    for path in paths:
        with open(path, encoding="utf-8") as handle:
            for line in handle:
                if line.strip():
                    rows.append(json.loads(line))
    groups = defaultdict(list)
    for row in rows:
        groups[(row["engine"], row.get("label", ""), bool(row.get("real")))].append(row)
    points = sorted({s["at_s"] for r in rows for s in (r.get("samples") or [])})

    head = "| engine | configuration | account | runs | ready s |"
    sep = "| --- | --- | --- | --- | --- |"
    for at in points:
        head += f" working set @{at}s | private @{at}s | processes | CPU s |"
        sep += " --- | --- | --- | --- |"
    head += " ws spread |"
    sep += " --- |"
    print(head)
    print(sep)

    for key in sorted(groups):
        engine, label, real = key
        runs = groups[key]
        line = f"| {engine} | {label or 'default'} | {'real' if real else 'logged out'} | {len(runs)}"
        line += f" | {fmt(med([r.get('ready_s') for r in runs]), 0)}"
        spread = ""
        for at in points:
            def pick(field):
                return [s.get(field) for r in runs for s in (r.get("samples") or []) if s["at_s"] == at]
            def whole(values):
                v = med(values)
                return "-" if v is None else str(int(round(v)))
            line += (
                f" | {whole(pick('ws_mb'))} | {whole(pick('private_mb'))}"
                f" | {whole(pick('processes'))} | {whole(pick('cpu_s'))}"
            )
            lo = [v for v in pick("ws_mb") if v is not None]
            spread = "single run" if len(lo) < 2 else f"{int(round(min(lo)))}-{int(round(max(lo)))}"
        line += f" | {spread} |"
        print(line)


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "--markdown"]
    paths = args or ["bench-engines.jsonl"]
    if "--markdown" in sys.argv:
        markdown(paths)
        sys.exit(0)
    sys.exit(main(paths))
