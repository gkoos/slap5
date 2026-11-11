"""Fits the class values to game outcomes instead of to Rapfi's depth-8 number.

Every other fit in this directory targets a proxy: Rapfi's own search score of a position. This one targets
the thing the engine is for. Each dumped position is labelled from the point of view of the side to move, by
the result of the game it came from, and the six class differences plus the tempo sign are fitted by least
squares to that label.

Two honest limitations. All the positions of one game share its label, so the independent sample is the games,
not the positions - which is why the split is by game and why the weights are reported with the number of
games next to them. And the positions were produced by whatever build played the game, so a corpus pooled over
several builds samples the position space more widely than any one of them but is off-policy for all of them.

Usage: python engine/scripts/fit_outcome.py [--root logs] [--holdout 3] [--dump]
Reports the weights, then how well three vectors rank the held-out games' positions: the shipped values, the
values currently in eval.rs, and the fitted ones.
"""

import argparse
import csv
import glob
import math
import os
import re
import subprocess
import sys

CLASSES = 6
SHIPPED = [3240, 1620, 162, 162, 27, 9]
IN_ENGINE = [3240, 540, 445, 140, 140, 104]  # engine/src/eval.rs
HEAD = re.compile(r"^(\S+)#(\d+) us=(\d+) static=(-?\d+) stones=(.*)$")


def read_features(path):
    out = {}
    with open(path, encoding="utf-8", errors="replace") as fh:
        for line in fh:
            m = HEAD.match(line.rstrip("\n"))
            if not m:
                continue
            stem, ply, us = m.group(1), int(m.group(2)), int(m.group(3))
            feats = {}
            for tok in m.group(5).split():
                if "=" in tok:
                    k, v = tok.split("=", 1)
                    feats[k] = float(v)
            out[(stem, ply)] = {"us": us, "f": feats}
    return out


def read_outcomes(run):
    """{(stem, slap5 moved first): +1 win / -1 loss} from the run's results.csv."""
    found = glob.glob(os.path.join(run, "**", "results.csv"), recursive=True)
    if not found:
        return None
    out = {}
    with open(found[0], newline="", encoding="utf-8", errors="replace") as fh:
        for row in csv.DictReader(fh):
            if row.get("Outcome") not in ("win", "loss"):
                continue
            first = row.get("Slap5First") == "True"
            stem = "p%02d_%s_first" % (int(row["Pair"]), "slap5" if first else "opponent")
            out[(stem, first)] = 1.0 if row["Outcome"] == "win" else -1.0
    return out


def build_corpus(root, dump_bin):
    """Every position of every recorded game that has a result, labelled from the mover's side."""
    # Recursive: a run is wherever the .psq files are - one level (logs/<run>), two (logs/<leg>/<run>) or
    # deeper (logs/pool-chain/<opponent>/<run>) - so find the files and keep the directories that hold them.
    runs = sorted({os.path.dirname(p) for p in glob.glob(os.path.join(root, "**", "*.psq"), recursive=True)})
    rows = []
    for run in sorted(runs):
        outcomes = read_outcomes(run)
        if not outcomes:
            continue
        text = subprocess.run([dump_bin, run, "1", "6"], capture_output=True, text=True).stdout
        dump = os.path.join(run, "dump.txt")
        with open(dump, "w", encoding="ascii") as fh:
            fh.write(text)
        game = os.path.basename(run)
        for (stem, ply), row in read_features(dump).items():
            first = stem.endswith("slap5_first")
            if (stem, first) not in outcomes:
                continue
            mover_is_slap5 = (row["us"] == 0) == first
            y = outcomes[(stem, first)] * (1.0 if mover_is_slap5 else -1.0)
            rows.append({"game": game + "/" + stem, "y": y, "f": row["f"]})
    return rows


def solve(a, b):
    n = len(b)
    m = [a[i][:] + [b[i]] for i in range(n)]
    for col in range(n):
        piv = max(range(col, n), key=lambda r: abs(m[r][col]))
        if abs(m[piv][col]) < 1e-12:
            raise ValueError("singular system")
        m[col], m[piv] = m[piv], m[col]
        pv = m[col][col]
        for r in range(n):
            if r != col and m[r][col] != 0.0:
                f = m[r][col] / pv
                for c in range(col, n + 1):
                    m[r][c] -= f * m[col][c]
    return [m[i][n] / m[i][i] for i in range(n)]


def columns():
    return [f"d{c}" for c in range(CLASSES)] + ["tp"]


# The candidate terms a fit can be given on top of the classes, as (name, column) pairs.
def extra_columns(names):
    out = []
    for name in names:
        if name == "zo":
            out += [f"zo{c}" for c in range(CLASSES)]
        elif name in ("zf", "zt", "zc", "zcen", "zx"):
            out.append(name)
        else:
            raise SystemExit(f"unknown term {name}; one of zo, zf, zt, zc, zcen, zx")
    return out


def value(feats, col):
    if col == "tp":
        return feats["tp"]
    if col == "zf":
        return feats["xfu"] - feats["xft"]
    if col == "zt":
        return feats["xtu"] - feats["xtt"]
    if col == "zc":
        return feats["shu"] - feats["sht"]
    if col == "zcen":
        return feats["cenu"] - feats["cent"]
    if col == "zx":
        return (feats["xuu"] - feats["xau"]) - (feats["xut"] - feats["xat"])
    if col.startswith("zo"):
        c = col[2:]
        return feats[f"ou{c}"] - feats[f"ot{c}"]
    c = col[1:]
    return feats[f"gu{c}"] - feats[f"gt{c}"]


def least_squares(rows, cols, ridge):
    """Standardized least squares with an L2 penalty, returned as raw coefficients in the label's units. The
    penalty is not optional: the five class is constant zero in any dumped corpus (positions where either side
    can complete five are left out), so its column has no variance to divide by."""
    n, p = len(rows), len(cols)
    xs = [[value(r["f"], c) for c in cols] for r in rows]
    ys = [r["y"] for r in rows]
    mu = [sum(x[j] for x in xs) / n for j in range(p)]
    sd = []
    for j in range(p):
        v = sum((x[j] - mu[j]) ** 2 for x in xs) / n
        sd.append(math.sqrt(v) if v > 1e-12 else 1.0)
    zs = [[(x[j] - mu[j]) / sd[j] for j in range(p)] for x in xs]
    my = sum(ys) / n
    yc = [y - my for y in ys]
    a = [[sum(zs[i][j] * zs[i][k] for i in range(n)) + (ridge if j == k else 0.0) for k in range(p)] for j in range(p)]
    b = [sum(zs[i][j] * yc[i] for i in range(n)) for j in range(p)]
    w = solve(a, b)
    raw = [w[j] / sd[j] for j in range(p)]
    intercept = my - sum(raw[j] * mu[j] for j in range(p))
    return raw, intercept


def vector_score(coef, cols, feats):
    return sum(w * value(feats, c) for w, c in zip(coef, cols))


def auc(scores, labels):
    """Rank-sum AUC: how often a won position scores above a lost one."""
    order = sorted(range(len(scores)), key=lambda i: scores[i])
    ranks = [0.0] * len(scores)
    i = 0
    while i < len(order):
        j = i
        while j + 1 < len(order) and scores[order[j + 1]] == scores[order[i]]:
            j += 1
        avg = (i + j) / 2.0 + 1.0
        for k in range(i, j + 1):
            ranks[order[k]] = avg
        i = j + 1
    pos = [ranks[i] for i in range(len(scores)) if labels[i] > 0]
    npos, nneg = len(pos), len(scores) - len(pos)
    if not npos or not nneg:
        return float("nan")
    return (sum(pos) - npos * (npos + 1) / 2.0) / (npos * nneg)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--root", default="logs")
    ap.add_argument("--dump", default=os.path.join("target", "release", "examples", "eval_dump.exe"))
    ap.add_argument("--holdout", type=int, default=3, help="every n-th game is held out")
    ap.add_argument("--ridge", type=float, default=1.0)
    ap.add_argument("--extra", default="", help="candidate terms to add: zo,zf,zt,zc,zcen,zx")
    args = ap.parse_args()

    rows = build_corpus(args.root, args.dump)
    if len(rows) < 200:
        raise SystemExit(f"only {len(rows)} labelled positions; check the dump path {args.dump}")
    games = sorted({r["game"] for r in rows})
    hold = {g for i, g in enumerate(games) if i % args.holdout == 0}
    train = [r for r in rows if r["game"] not in hold]
    test = [r for r in rows if r["game"] in hold]
    wins = sum(1 for r in test if r["y"] > 0)
    print(f"positions {len(rows)} from {len(games)} games: fit {len(train)} positions / {len(games) - len(hold)} games, "
          f"hold-out {len(test)} positions / {len(hold)} games ({wins} won by the side to move, {len(test) - wins} lost)")

    base = columns()
    cols = base + extra_columns([n for n in args.extra.split(",") if n])
    coef, intercept = least_squares(train, cols, args.ridge)
    print(f"\nfitted per unit of each feature (intercept {intercept:+.5f}):")
    for c, w in zip(cols, coef):
        print(f"  {c:<5} {w:+.5f}")

    # Every vector is on the engine's convention: eval = (sum VALUE * diff) / 2 + TEMPO * tp.
    refs = {
        "shipped": ([v / 2.0 for v in SHIPPED] + [45.0], base),
        "in eval.rs": ([v / 2.0 for v in IN_ENGINE] + [45.0], base),
        "outcome fit": (coef, cols),
    }
    print("\nhow well each vector ranks the held-out positions (1.0 = every won position above every lost one):")
    for name, (c, cs) in refs.items():
        scores = [vector_score(c, cs, r["f"]) for r in test]
        print(f"  {name:<12} AUC {auc(scores, [r['y'] for r in test]):.4f}")

    spread = lambda vals: (sum((v - sum(vals) / len(vals)) ** 2 for v in vals) / len(vals)) ** 0.5
    shipped_scores = [vector_score(refs["shipped"][0], base, r["f"]) for r in rows]
    engine_scores = [vector_score(refs["in eval.rs"][0], base, r["f"]) for r in rows]
    fit_scores = [vector_score(coef, cols, r["f"]) for r in rows]
    print(f"\nspread {spread(shipped_scores):.0f} shipped, {spread(engine_scores):.0f} in eval.rs, "
          f"{spread(fit_scores):.4f} fitted")
    factor = spread(engine_scores) / spread(fit_scores)
    print(f"factor {factor:.1f} puts the fit on the spread the pruning margins were tuned against, "
          f"and gives integer weights")
    print("engine form: VALUE = [3240, %s]  TEMPO = %d" % (
        ", ".join(str(round(w * 2 * factor)) for w in coef[1:CLASSES]), round(coef[CLASSES] * factor)))
    print(f"the intercept ({intercept * factor:.0f} scaled) is a claim about whoever moves, not about a position, "
          f"and has no place in the engine's form")


if __name__ == "__main__":
    main()

