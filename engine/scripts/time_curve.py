"""Where does thinking time change slap5's move? Agreement with Rapfi's move, per budget and game phase.

  sample   game records (pNN_slap5_first/opponent_first.psq exports) -> positions where slap5 is to move
  rapfi    positions -> Rapfi's move at a long time, through the Gomocup protocol (the reference)
  analyse  positions + Rapfi moves + `cargo run --release --example time_curve` output -> tables

One long slap5 search per position stands in for every shorter budget: the search is deterministic, so a
search given B ms plays the last iteration that completed by B and that started by B/2 (the iteration
loop does not start one after half the budget).

Usage:
  python engine/scripts/time_curve.py sample <openings file> <out positions> <per phase> <run dir>...
  python engine/scripts/time_curve.py rapfi <positions> <rapfi exe> <ms> <out moves>
  python engine/scripts/time_curve.py analyse <positions> <rapfi moves> <time_curve output>
"""
import os
import random
import re
import subprocess
import sys
from collections import defaultdict

PHASES = (("early", 4, 15), ("middle", 16, 31), ("late", 32, 225))
BUDGETS_MS = (250, 500, 1000, 2000, 4000, 8000, 16000, 30000)


def phase_of(stones):
    for name, lo, hi in PHASES:
        if lo <= stones <= hi:
            return name
    return None


def pair_openings(run):
    """Line of the openings file each pair of a run was played from, as the run's run-info.txt records it."""
    path = os.path.join(run, "run-info.txt")
    if not os.path.exists(path):
        raise SystemExit(f"{run}: no run-info.txt, so its pair numbers cannot be mapped to openings")
    for line in open(path):
        m = re.match(r"pairs: \d+; openings: ([\d,]+)\s*$", line)
        if m:
            return [int(i) for i in m.group(1).split(",")]
    raise SystemExit(f"{path}: no `pairs: N; openings: ...` line")


def sample(openings_file, out, per_phase, runs):
    stones_per_opening = [len([t for t in l.split(",") if t.strip()]) // 2
                          for l in open(openings_file) if l.strip()]
    found = {name: [] for name, _, _ in PHASES}
    seen = set()
    for run in runs:
        mapping = pair_openings(run)
        for name in sorted(os.listdir(run)):
            m = re.match(r"p(\d\d)_(slap5|opponent)_first\.psq$", name)
            if not m:
                continue
            ours = 0 if m.group(2) == "slap5" else 1
            opening = stones_per_opening[mapping[int(m.group(1))]]
            moves = []
            for line in open(os.path.join(run, name)):
                p = line.strip().split(",")
                if len(p) == 3 and p[0].isdigit() and p[1].isdigit():
                    moves.append((int(p[0]) - 1, int(p[1]) - 1))
            for k in range(opening, len(moves)):
                if k % 2 != ours:
                    continue
                key = tuple(moves[:k])
                ph = phase_of(k)
                if ph and key not in seen:
                    seen.add(key)
                    found[ph].append(key)
    rng = random.Random(1)
    with open(out, "w") as fh:
        n = 0
        for ph, _, _ in PHASES:
            pick = rng.sample(found[ph], min(per_phase, len(found[ph])))
            for moves in pick:
                fh.write(f"{n} {ph} {len(moves)} {len(moves) % 2} " + " ".join(f"{x},{y}" for x, y in moves) + "\n")
                n += 1
            print(f"{ph}: {len(found[ph])} candidate positions, {min(per_phase, len(found[ph]))} sampled")


def read_positions(path):
    out = []
    for line in open(path):
        p = line.split()
        out.append((int(p[0]), p[1], int(p[2]), int(p[3]), [tuple(map(int, m.split(","))) for m in p[4:]]))
    return out


def rapfi(positions, exe, ms, out):
    proc = subprocess.Popen([exe], cwd=os.path.dirname(os.path.abspath(exe)), stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, text=True, bufsize=1)

    def send(s):
        proc.stdin.write(s + "\n")
        proc.stdin.flush()

    def until(pattern):
        while True:
            line = proc.stdout.readline()
            if not line:
                raise RuntimeError("rapfi exited")
            line = line.strip()
            if re.match(pattern, line):
                return line

    send("START 15")
    until(r"^OK")
    with open(out, "w") as fh:
        for i, (pid, _, _, side, moves) in enumerate(read_positions(positions)):
            if i:
                send("RESTART")
                until(r"^OK")
            send("INFO rule 0")
            send(f"INFO timeout_turn {ms}")
            send("INFO timeout_match 0")
            send("INFO time_left 2147483647")
            send("BOARD")
            for k, (x, y) in enumerate(moves):
                send(f"{x},{y},{1 if k % 2 == side else 2}")
            send("DONE")
            move = until(r"^\d+,\d+$")
            fh.write(f"{pid} {move}\n")
            fh.flush()
            print(f"{pid} {move}", flush=True)
    send("END")


def analyse(positions, rapfi_moves, curve):
    pos = {p[0]: p for p in read_positions(positions)}
    ref = {int(l.split()[0]): l.split()[1] for l in open(rapfi_moves) if l.strip()}
    iters = defaultdict(list)
    final = {}
    raw = open(curve, "rb").read()
    # PowerShell's redirection writes UTF-16.
    text = raw.decode("utf-16") if raw[:2] in (b"\xff\xfe", b"\xfe\xff") else raw.decode("utf-8", "replace")
    for line in text.splitlines():
        p = line.split()
        if len(p) == 6 and p[1] == "iter":
            iters[int(p[0])].append((int(p[2]), int(p[3]), p[4]))
        elif len(p) == 6 and p[1] == "final":
            final[int(p[0])] = (int(p[2]), p[3], int(p[4]))

    def move_at(pid, budget):
        its = iters[pid]
        if not its:
            return final[pid][1]
        move, prev_done = its[0][2], its[0][1]
        for depth, done, mv in its[1:]:
            if prev_done * 2 > budget or done > budget:
                break
            move, prev_done = mv, done
        return move

    ids = [i for i in pos if i in ref and i in final]
    instant = [i for i in ids if not iters[i]]
    print(f"positions with a reference and a search: {len(ids)}; decided before any iteration "
          f"(a five on the board): {len(instant)}, agreeing with Rapfi: {sum(final[i][1] == ref[i] for i in instant)}")
    # Where our long search already reports a forced win or loss, every move scores the same to it, so
    # agreement there measures nothing about time; those positions are counted, not tabled.
    decided = [i for i in ids if abs(final[i][2]) >= 29000]
    print(f"positions our {max((final[i][0] for i in ids), default=0) / 1000:g} s search calls decided (forced win or loss): "
          f"{len(decided)}, left out of the tables")
    ids = [i for i in ids if i not in decided]
    print()
    print("agreement with Rapfi's move, by budget")
    print(f"{'phase':<8} {'n':>4} " + " ".join(f"{b / 1000:>6g}s" for b in BUDGETS_MS))
    for ph in [p[0] for p in PHASES] + ["all"]:
        sel = [i for i in ids if ph == "all" or pos[i][1] == ph]
        if not sel:
            continue
        cells = [100.0 * sum(move_at(i, b) == ref[i] for i in sel) / len(sel) for b in BUDGETS_MS]
        print(f"{ph:<8} {len(sel):>4} " + " ".join(f"{c:>6.1f}%" for c in cells))
    differ = sum(move_at(i, BUDGETS_MS[0]) != move_at(i, BUDGETS_MS[-1]) for i in ids)
    print(f"the move at {BUDGETS_MS[0] / 1000:g} s differs from the move at {BUDGETS_MS[-1] / 1000:g} s in "
          f"{100.0 * differ / len(ids):.1f}% of {len(ids)} positions")
    print()
    print("move changes after it has been stable: of iterations whose move equalled the previous k-1 iterations'")
    print("moves, how often the last iteration completed within 30 s plays a different move")
    stats = defaultdict(lambda: [0, 0])
    for i in ids:
        its = iters[i]
        if not its:
            continue
        last = its[-1][2]
        run = 0
        for j, (_, _, mv) in enumerate(its):
            run = run + 1 if j and mv == its[j - 1][2] else 1
            if j == len(its) - 1:
                break
            k = min(run, 4)
            stats[k][0] += 1
            stats[k][1] += mv != last
    for k in sorted(stats):
        n, changed = stats[k]
        print(f"  stable for {k}{'+' if k == 4 else ''} iteration(s): {n:>5} iterations, changed later in {100.0 * changed / n:5.1f}%")


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "sample":
        sample(sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5:])
    elif cmd == "rapfi":
        rapfi(sys.argv[2], sys.argv[3], int(sys.argv[4]), sys.argv[5])
    elif cmd == "analyse":
        analyse(sys.argv[2], sys.argv[3], sys.argv[4])
    else:
        sys.exit(__doc__)
