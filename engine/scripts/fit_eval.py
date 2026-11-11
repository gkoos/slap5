"""Prices evaluation changes against Rapfi's labels on positions that are already labelled, so a change to
the evaluation can be measured without playing a game. Reads the features `examples/eval_dump.rs` writes and
the labels `engine/scripts/rapfi_labels.ps1` produces, and reports how well a least-squares fit of each
candidate feature set predicts Rapfi, on games held out of the fit.

The split is by game, never by position: the 50 plies of one game are not independent samples. Mate labels
are dropped, since a count model has no vocabulary for them. Correlation is the reported statistic because it
is invariant to the scale and offset that the pruning thresholds are tuned to.

Usage: python engine/scripts/fit_eval.py --dump <dump.txt> --labels <evals.tsv> [--holdout 3] [--ridge 1.0]
Features (u the mover, t the other): g{c} class c as groups per direction, v{c} as cells, cen centrality,
adj same-side neighbour pairs, tp +1 when board player 0 moves. Classes, strongest first: 0 five, 1 open
four, 2 four, 3 open three, 4 blocked three, 5 two.
"""

import argparse
import collections
import math
import re

HEAD = re.compile(r"^(\S+)#(\d+) us=(\d+) static=(-?\d+) stones=(.*)$")
CLASSES = 6
CLASS_NAMES = ["five", "open4", "four", "open3", "block3", "two"]
# engine/src/eval.rs, so the baseline row reproduces the dumped `static` column. Override with --weights.
ENGINE_VALUE = [3240, 540, 445, 140, 140, 104]
ENGINE_TEMPO = 45


def read_text(path):
    """Decode by BOM: PowerShell 5.1's `>` writes UTF-16, 7's writes UTF-8."""
    data = open(path, "rb").read()
    if data[:2] in (b"\xff\xfe", b"\xfe\xff"):
        return data.decode("utf-16")
    return data.decode("utf-8", "replace")


def read_features(path):
    """{(name, ply): {'name':, 'us':, 'static':, feature->value}}"""
    out = {}
    for line in read_text(path).splitlines():
        m = HEAD.match(line.rstrip("\n"))
        if not m:
            continue
        name, ply, us, static, rest = m.group(1), int(m.group(2)), int(m.group(3)), int(m.group(4)), m.group(5)
        feats = {}
        for tok in rest.split():
            if "=" in tok:
                k, v = tok.split("=", 1)
                feats[k] = float(v)
        out[(name, ply)] = {"name": name, "us": us, "static": float(static), "f": feats}
    return out


def read_labels(path, mates):
    """{(name, ply): (kind, value)} for depth-8 labels; `kind` is `num` or `mate`. `mates` is `drop`,
    `flat:<score>` or `dist:<base>:<slope>`: the mate rows are the only evidence about the decisive classes,
    so they can be kept as a bounded score instead of dropped."""
    kind, mate_value = "drop", None
    if mates.startswith("flat:"):
        kind, mate_value = "flat", float(mates.split(":")[1])
    elif mates.startswith("dist:"):
        kind, mate_value = "dist", tuple(float(x) for x in mates.split(":")[1:3])
    out = {}
    for line in read_text(path).splitlines():
        if line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) < 6 or parts[5] != "8":
            continue
        key, val = (parts[0], int(parts[1])), parts[4]
        if re.fullmatch(r"-?\d+", val):
            out[key] = ("num", float(val))
        elif kind != "drop" and re.fullmatch(r"[+-]M\d+", val):
            sign = 1.0 if val[0] == "+" else -1.0
            n = int(val[2:])
            score = mate_value if kind == "flat" else max(500.0, mate_value[0] - mate_value[1] * n)
            out[key] = ("mate", sign * score)
    return out


def spec_columns(spec):
    """Column names for a feature set. `v` adds the cell-count basis, `p` the positional terms, `s` the
    strategic ones in split form (direction overlap per class, directions holding a four or an open three,
    shifted neighbour pairs), `d` the difference basis `gu{c} - gt{c}` instead of pricing the sides
    separately. The capital letters pick the strategic terms in *difference* form, which is what the engine
    can implement: `O` overlap, `F` directions with a four, `T` directions with an open three, `C` neighbour
    pairs."""
    flags = spec[1:]
    if "d" in flags:
        cols = [f"d{c}" for c in range(CLASSES)]
    else:
        cols = []
        for side in "ut":
            cols += [f"g{side}{c}" for c in range(CLASSES)]
            if "v" in flags:
                cols += [f"v{side}{c}" for c in range(CLASSES)]
            if "p" in flags:
                cols += [f"cen{side}", f"adj{side}"]
            if "s" in flags:
                cols += [f"o{side}{c}" for c in range(CLASSES)]
                cols += [f"xf{side}", f"xt{side}", f"sh{side}"]
    if "O" in flags:
        cols += [f"zo{c}" for c in range(CLASSES)]
    if "F" in flags:
        cols.append("zf")
    if "T" in flags:
        cols.append("zt")
    if "C" in flags:
        cols.append("zc")
    if "P" in flags:
        cols.append("zcen")
    if "X" in flags:
        cols.append("zx")
    cols.append("tp")
    return cols


def value(feats, col):
    if col.startswith("d"):
        c = col[1:]
        return feats[f"gu{c}"] - feats[f"gt{c}"]
    if col.startswith("z"):
        if col == "zf":
            return feats["xfu"] - feats["xft"]
        if col == "zt":
            return feats["xtu"] - feats["xtt"]
        if col == "zc":
            return feats["shu"] - feats["sht"]
        if col == "zcen":
            return feats["cenu"] - feats["cent"]
        if col == "zx":
            # cells creating shapes of different classes in different directions, our side minus theirs
            return (feats["xuu"] - feats["xau"]) - (feats["xut"] - feats["xat"])
        c = col[2:]
        return feats[f"ou{c}"] - feats[f"ot{c}"]
    return feats[col]


def solve(a, b):
    """Gauss-Jordan with partial pivoting; the system is at most ~35 columns wide."""
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


def ridge_fit(rows, cols, ridge):
    """Least squares with an L2 penalty on standardized columns, in the labels' units."""
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
    return raw, my - sum(raw[j] * mu[j] for j in range(p))


def predict(rows, cols, coef, intercept):
    return [sum(coef[j] * value(r["f"], c) for j, c in enumerate(cols)) + intercept for r in rows]


def stats(pred, target):
    """Pearson r, the slope of the target on the prediction, sign agreement, the same after both are
    centred, and the prediction's spread and mean. No R2: with an intercept it is r squared, without one
    the offset dominates."""
    n = len(pred)
    mp, mt = sum(pred) / n, sum(target) / n
    spt = sum((pred[i] - mp) * (target[i] - mt) for i in range(n))
    spp = sum((pred[i] - mp) ** 2 for i in range(n))
    stt = sum((target[i] - mt) ** 2 for i in range(n))
    r = spt / math.sqrt(spp * stt) if spp > 0 and stt > 0 else float("nan")
    return {
        "r": r,
        "slope": spt / spp if spp > 0 else float("nan"),
        "agree": 100.0 * sum(1 for i in range(n) if (pred[i] > 0) == (target[i] > 0)) / n,
        "agreec": 100.0 * sum(1 for i in range(n) if (pred[i] - mp > 0) == (target[i] - mt > 0)) / n,
        "sd": math.sqrt(spp / n),
        "mean": mp,
    }


def engine_score(row):
    # eval.rs divides with i32 `/`, which truncates toward zero.
    diff = sum(ENGINE_VALUE[c] * (row["f"][f"gu{c}"] - row["f"][f"gt{c}"]) for c in range(CLASSES))
    return int(diff / 2) + ENGINE_TEMPO * row["f"]["tp"]



def spread(vals):
    m = sum(vals) / len(vals)
    return math.sqrt(sum((v - m) ** 2 for v in vals) / len(vals))


def isotonic_desc(vals):
    """Pool adjacent violators so the sequence is non-increasing. The class ladder - a five beats an open
    four beats a four - is an assumption about the game, not something to fit, so a fit that crosses two
    neighbouring classes is projected back onto it."""
    blocks = [[v] for v in vals]
    i = 0
    while i < len(blocks) - 1:
        if sum(blocks[i]) / len(blocks[i]) < sum(blocks[i + 1]) / len(blocks[i + 1]):
            blocks[i] += blocks[i + 1]
            del blocks[i + 1]
            i = max(0, i - 1)
        else:
            i += 1
    return [v for b in blocks for v in [sum(b) / len(b)] * len(b)]


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dump", required=True)
    ap.add_argument("--labels", required=True)
    ap.add_argument("--holdout", type=int, default=3, help="every n-th game is held out")
    ap.add_argument("--ridge", type=float, default=1.0)
    ap.add_argument("--specs", default="gd,g,gv,gvp")
    ap.add_argument("--weights", default="", help="score 'VALUE0,...,VALUE5;TEMPO[;CONST]' as given")
    ap.add_argument("--fixed", default="",
                    help="hold some classes, fit the rest: '3240,1620,-,-,-,-' holds five/open-four/four")
    ap.add_argument("--mates", default="drop", help="drop | flat:<score> | dist:<base>:<slope>")
    args = ap.parse_args()

    feats = read_features(args.dump)
    labels = read_labels(args.labels, args.mates)
    rows = [dict(feats[k], ply=k[1], y=labels[k][1], kind=labels[k][0])
            for k in sorted(feats) if k in labels]
    numeric = [r for r in rows if r["kind"] == "num"]
    if len(numeric) < 100:
        raise SystemExit(f"only {len(numeric)} positions have features and a plain depth-8 label")

    names = sorted({r["name"] for r in rows})
    hold = {n for i, n in enumerate(names) if i % args.holdout == 0}
    train = [r for r in rows if r["name"] not in hold]
    test = [r for r in numeric if r["name"] in hold]
    mate_test = [r for r in rows if r["kind"] == "mate" and r["name"] in hold]
    ytrain, ytest = [r["y"] for r in train], [r["y"] for r in test]
    print(f"positions with features and a depth-8 label: {len(numeric)} plain, "
          f"{len(rows) - len(numeric)} mate ({args.mates})   games: {len(names)}")
    print(f"split by game: fit {len(names) - len(hold)} games / {len(train)} positions, "
          f"hold-out {len(hold)} games / {len(test)} plain + {len(mate_test)} mate positions")
    print(f"labels: mean {sum(ytest) / len(ytest):.0f}  spread {spread(ytest):.0f}   "
          f"engine eval: mean {sum(r['static'] for r in test) / len(test):.0f}  spread {spread([r['static'] for r in test]):.0f}")
    print("mean of (own - opponent) count per class, the level the eval reads off:\n  " + "  ".join(
        f"{CLASS_NAMES[c]} {sum(r['f'][f'gu{c}'] - r['f'][f'gt{c}'] for r in numeric) / len(numeric):+.2f}"
        for c in range(CLASSES)))

    def mate_dir(pred_fn):
        """How often the eval agrees with Rapfi on the direction of a forced mate, the only reading the
        corpus gives of the decisive classes."""
        if not mate_test:
            return float("nan")
        return 100.0 * sum(1 for r in mate_test if (pred_fn(r) > 0) == (r["y"] > 0)) / len(mate_test)

    # The engine's vector read off the features; the same correlation read off the dumped `static` column
    # says whether the feature dump and the engine agree about every position.
    base_te = [engine_score(r) for r in test]
    b = stats(base_te, ytest)
    d = stats([r["static"] for r in test], ytest)
    print(f"\nengine VALUE/TEMPO on the hold-out: r={b['r']:.3f}  agree={b['agree']:.1f}%  "
          f"centred agree={b['agreec']:.1f}%  slope={b['slope']:.3f}  mean={b['mean']:.0f}  "
          f"mate dir={mate_dir(engine_score):.1f}%")
    print(f"  cross-check against the dumped static column: r={d['r']:.3f}  "
          f"({'identical' if abs(d['r'] - b['r']) < 1e-6 else 'MISMATCH'})")

    print(f"\n{'model':<6} {'p':>3} {'fit r':>7} {'hold r':>7} {'agree':>7} {'ctr agree':>10} "
          f"{'slope':>7} {'mean':>7} {'mate':>6}")
    results = []
    for spec in args.specs.split(","):
        cols = spec_columns(spec)
        coef, intercept = ridge_fit(train, cols, args.ridge)
        f = stats(predict(train, cols, coef, intercept), ytrain)
        h = stats(predict(test, cols, coef, intercept), ytest)
        md = mate_dir(lambda r: sum(coef[j] * value(r["f"], c) for j, c in enumerate(cols)) + intercept)
        results.append((spec, cols, coef, intercept, f, h))
        print(f"{spec:<6} {len(cols):>3} {f['r']:>7.3f} {h['r']:>7.3f} {h['agree']:>6.1f}% "
              f"{h['agreec']:>9.1f}% {h['slope']:>7.3f} {h['mean']:>7.0f} {md:>5.1f}%")
    print("(ridge {0}, columns standardized before the penalty)".format(args.ridge))

    if args.fixed:
        fixed = [None if x == "-" else float(x) for x in args.fixed.split(",")]
        if len(fixed) != CLASSES:
            raise SystemExit(f"--fixed needs {CLASSES} comma-separated values")

        def held(row):
            """A held value contributes value/2 per unit of `gu{c} - gt{c}` in the engine's convention."""
            return sum(fixed[c] / 2.0 * (row["f"][f"gu{c}"] - row["f"][f"gt{c}"])
                       for c in range(CLASSES) if fixed[c] is not None)

        cols = [f"d{c}" for c in range(CLASSES) if fixed[c] is None] + ["tp"]
        # The capital flags of the first spec add the strategic terms in difference form, which is how they
        # would land in the engine next to fixed class values.
        flags = args.specs.split(",")[0][1:]
        if "O" in flags:
            cols += [f"zo{c}" for c in range(CLASSES)]
        if "F" in flags:
            cols.append("zf")
        if "T" in flags:
            cols.append("zt")
        if "C" in flags:
            cols.append("zc")
        if "P" in flags:
            cols.append("zcen")
        if "X" in flags:
            cols.append("zx")
        free_train = [dict(r, y=r["y"] - held(r)) for r in train]
        free_test = [dict(r, y=r["y"] - held(r)) for r in test]
        coef, intercept = ridge_fit(free_train, cols, args.ridge)
        tpred = [held(r) + p for r, p in zip(test, predict(free_test, cols, coef, intercept))]
        g = stats(tpred, ytest)
        whole = [None] * CLASSES
        for c in range(CLASSES):
            whole[c] = fixed[c] if fixed[c] is not None else 2.0 * coef[cols.index(f"d{c}")]
        print(f"\nheld VALUE={[fixed[c] for c in range(CLASSES)]}, fitted the rest:  r={g['r']:.3f}  "
              f"agree={g['agree']:.1f}%  centred agree={g['agreec']:.1f}%  slope={g['slope']:.3f}  "
              f"mean={g['mean']:.0f}  spread={g['sd']:.0f}  mate dir={mate_dir(lambda r: held(r) + sum(coef[j] * value(r['f'], c) for j, c in enumerate(cols)) + intercept):.1f}%")
        print(f"  whole vector in engine convention: VALUE={[round(v, 1) for v in whole]} "
              f"TEMPO={coef[cols.index('tp')]:.1f}  CONST={intercept:.1f}")
        print("  fitted terms: " + "  ".join(f"{c}={w:+.1f}" for c, w in zip(cols, coef)))

    # The projected variant, scaled back onto the engine's spread.
    cols = spec_columns("gd")
    coef, intercept = ridge_fit(train, cols, args.ridge)
    raw = [coef[cols.index(f"d{c}")] for c in range(CLASSES)]
    proj = raw[:1] + isotonic_desc(raw[1:])
    proj_full = proj + [coef[cols.index("tp")]]  # aligned with `cols`, which carries tempo after the classes
    fp = stats(predict(train, cols, proj_full, intercept), ytrain)
    hp = stats(predict(test, cols, proj_full, intercept), ytest)
    mdp = mate_dir(lambda r: sum(proj_full[j] * value(r["f"], c) for j, c in enumerate(cols)) + intercept)
    sd_ship = spread([engine_score(r) for r in rows])
    factor = sd_ship / spread(predict(rows, cols, proj_full, intercept))
    engine_value = [2.0 * proj[c] * factor for c in range(CLASSES)]
    engine_value[0] = 2.0 * engine_value[1]
    print(f"\nisotonic projection of the gd fit (mates {args.mates}):  fit r={fp['r']:.3f}  hold r={hp['r']:.3f}  "
          f"agree={hp['agree']:.1f}%  centred agree={hp['agreec']:.1f}%  mate dir={mdp:.1f}%")
    print(f"  as the engine reads it: VALUE={[round(v) for v in engine_value]}  TEMPO={round(coef[cols.index('tp')] * factor)}  "
          f"CONST={round(intercept * factor)}   (scale {factor:.3f} onto the shipped spread {sd_ship:.0f})")
    print(f"  raw ladder before the projection: {[round(2.0 * v, 1) for v in raw[1:]]}, five pinned at "
          f"2x the open four because the corpus holds no position with a five available")

    if args.weights:
        parts = args.weights.split(";")
        val = [float(x) for x in parts[0].split(",")]
        tempo = float(parts[1]) if len(parts) > 1 else 0.0
        const = float(parts[2]) if len(parts) > 2 else 0.0

        def given(row):
            diff = sum(val[c] * (row["f"][f"gu{c}"] - row["f"][f"gt{c}"]) for c in range(len(val)))
            return int(diff / 2) + tempo * row["f"]["tp"] + const  # truncating like eval.rs does

        g = stats([given(r) for r in test], ytest)
        gf = stats([given(r) for r in train], ytrain)
        print(f"\ngiven VALUE={val} TEMPO={tempo} CONST={const}:  fit r={gf['r']:.3f}  r={g['r']:.3f}  "
              f"agree={g['agree']:.1f}%  centred agree={g['agreec']:.1f}%  slope={g['slope']:.3f}  "
              f"mean={g['mean']:.0f}  spread={g['sd']:.0f}  mate dir={mate_dir(given):.1f}%")

    spec, cols, coef, intercept, f, h = max(results, key=lambda x: x[5]["r"])
    print(f"\nbest by hold-out r: {spec}  r={h['r']:.3f} (shipped {b['r']:.3f}, "
          f"{(h['r'] - b['r']) * 100:+.1f} points of r)  agree={h['agree']:.1f}%  "
          f"centred agree={h['agreec']:.1f}%")
    print(f"{'term':<8}{'raw':>12}{'scaled':>12}")
    for c, w in zip(cols, coef):
        print(f"{c:<8}{w:>12.3f}{'':>12}")
    print(f"{'const':<8}{intercept:>12.3f}")

    # Match the engine's spread: the pruning margins are absolute numbers in the eval's units.
    sd_ship = spread([engine_score(r) for r in rows])
    sd_fit = spread(predict(rows, cols, coef, intercept))
    factor = sd_ship / sd_fit
    print(f"\nscale: shipped spread {sd_ship:.0f}, fitted {sd_fit:.0f}, factor to restore the shipped "
          f"spread {factor:.3f}")
    print(f"scaled coefficients (multiply every term and the constant by {factor:.3f}):")
    for c, w in zip(cols, coef):
        print(f"{c:<8}{w * factor:>12.3f}")
    print(f"{'const':<8}{intercept * factor:>12.3f}")
    print("\nRust: VALUE[c] = 2*w(d c), TEMPO = w(tp); a `v`/`cen`/`adj` term needs code in eval.rs.")


if __name__ == "__main__":
    main()

