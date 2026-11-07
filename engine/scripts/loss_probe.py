#!/usr/bin/env python3
"""Where do our lost games turn?

The colour split says the engine plays the second player badly (47% of points as first mover against
26% as second, against pela). This finds the move where a lost game goes wrong, so that gap can be
aimed at something: an opening problem, a defensive lapse, or a collapse inside a forced chain.

For every recorded game we lost, it replays the .psq record to establish the winner, reads our own
engine log from the same run directory (the log named after the binary that run-info.txt records), and
reports the first of our moves after which our reported score falls by more than a threshold. Output is
one line per game plus a histogram by ply and by colour.

It also reports which opening list each run used (run-info.txt's openings hash) and warns if a corpus
mixes lists, because a per-opening record pooled across two lists compares positions that are not the
same position - the pair index is only meaningful together with the file it indexes.

Usage: python engine/scripts/loss_probe.py [logs_dir] [threshold]
  logs_dir   default: logs
  threshold  default: 400 points of score dropped over one of our moves plus the reply

Nothing is written; it only reads. Games whose log is missing or whose lines do not line up are
reported as skipped rather than guessed at.
"""
import os
import re
import sys
from collections import Counter

SCORE = re.compile(r"^DEBUG depth (\d+) score (-?\d+)")


def replay(psq_path):
    """Return the winner (0 or 1) or None, by replaying the record. Player 0 moves first."""
    board = {}
    lines = [ln.strip() for ln in open(psq_path, encoding="utf-8", errors="replace") if ln.strip()]
    if not lines:
        return None
    ply = 0
    for line in lines[1:]:  # first line is the header: "Piskvorky 15x15, 4:4, 0"
        parts = line.split(",")
        if len(parts) < 2:
            continue
        try:
            x, y = int(parts[0]), int(parts[1])
        except ValueError:
            continue
        player = ply % 2
        board[(x, y)] = player
        ply += 1
        for dx, dy in ((1, 0), (0, 1), (1, 1), (1, -1)):
            run = 1
            for step in (1, -1):
                cx, cy = x + dx * step, y + dy * step
                while board.get((cx, cy)) == player:
                    run += 1
                    cx, cy = cx + dx * step, cy + dy * step
            if run >= 5:
                return player
    return None


def our_log(run_dir, run_info_path):
    """The log of our engine in that run, or None. run-info.txt names the engine binary."""
    try:
        info = open(run_info_path, encoding="utf-8", errors="replace").read()
    except OSError:
        return None
    for line in info.splitlines():
        if line.startswith("engine: "):
            exe = os.path.basename(line[len("engine: "):].strip())
            candidate = os.path.join(run_dir, exe + ".log")
            if os.path.exists(candidate):
                return candidate
    return None


def opponent_and_build(run_info_path):
    """Opponent basename and engine hash from run-info.txt, so openings are not averaged across opponents."""
    opponent, build = "?", "?"
    try:
        for line in open(run_info_path, encoding="utf-8", errors="replace"):
            if line.startswith("opponent: "):
                opponent = os.path.basename(line[len("opponent: "):].strip())
            elif line.startswith("engine hash: "):
                build = line[len("engine hash: "):].strip()
    except OSError:
        pass
    return opponent, build


def openings_identity(run_info_path):
    """The openings file and its hash from run-info.txt, so runs that used a different list are visible."""
    path, digest = "?", "?"
    try:
        for line in open(run_info_path, encoding="utf-8", errors="replace"):
            if line.startswith("openings file: "):
                path = line[len("openings file: "):].strip()
            elif line.startswith("openings hash: "):
                digest = line[len("openings hash: "):].strip().split(";")[0].strip()
    except OSError:
        pass
    return path, digest


def records_in_play_order(records):
    """The records in the order the games were played, so the nth log section belongs to the nth record.

    File names carry the pair index and whose move it was, not the order. Sorting them alphabetically used
    to match the play order - `high5` sorts before `opponent` - but `slap5` sorts after it, so for every run
    since the rename the two games of each pair came out swapped and each game was read against its
    sibling's scores. The engine plays our game first in each pair, so sorting by pair and then by whose
    move it was reconstructs the order for both namings.
    """
    def key(name):
        pair = int(name[1:3]) if name[1:3].isdigit() else 0
        return (pair, 1 if name.endswith("_opponent_first.psq") else 0)

    return sorted(records, key=key)


def openings_by_pair(run_info_path):
    """The opening index used for each pair, from run-info.txt's `pairs: N; openings: a,b,c,...` line."""
    try:
        for line in open(run_info_path, encoding="utf-8", errors="replace"):
            if line.startswith("pairs:") and "openings:" in line:
                tail = line.split("openings:", 1)[1]
                return [int(t) for t in re.findall(r"\d+", tail)]
    except OSError:
        pass
    return []


def games_from_log(log_path):
    """Our own move scores per game, in the order the games were played.

    The engines are relaunched for every game but write to the same file, so one log holds the whole run:
    Piskvork's `START` marks the beginning of each game and is the boundary. Games appear in the same order
    as the records (`p00_high5_first`, `p00_opponent_first`, `p01_high5_first`, ...), which is their sorted
    order, so the nth section belongs to the nth record.
    """
    games = []
    scores = []
    started = False
    for line in open(log_path, encoding="utf-8", errors="replace"):
        text = line.strip()
        if text.startswith("START"):
            if started:
                games.append(scores)
            scores = []
            started = True
            continue
        m = SCORE.match(text)
        if m:
            scores.append(int(m.group(2)))
    if started:
        games.append(scores)
    return games


def main():
    logs_dir = sys.argv[1] if len(sys.argv) > 1 else "logs"
    threshold = int(sys.argv[2]) if len(sys.argv) > 2 else 400

    rows = []
    skipped = 0
    skips = []
    losses_by_colour = Counter()
    played = Counter()
    lost = Counter()
    # Aggregated over opponents, so an opening can be compared as white and as black side by side.
    won_agg = Counter()
    lost_agg = Counter()
    played_agg = Counter()
    opening_lists = Counter()
    for run_dir, _, files in os.walk(logs_dir):
        info = os.path.join(run_dir, "run-info.txt")
        if os.path.exists(info):
            opening_lists[openings_identity(info)[1]] += 1
        log = our_log(run_dir, info) if os.path.exists(info) else None
        records = records_in_play_order([n for n in files if n.endswith(".psq")])
        if log is None:
            skips.append((os.path.basename(run_dir), "no engine log", len(records)))
            skipped += len(records)
            continue
        games = games_from_log(log)
        if len(games) < len(records):
            skips.append((os.path.basename(run_dir), f"log has {len(games)} games against {len(records)} records", len(records)))
            skipped += len(records)
            continue
        if len(games) > len(records):
            # One extra trailing section is the usual case: a game killed by hand, whose record was never
            # written. Dropping it keeps the run, which is worth doing - refusing a whole pool over a single
            # unmatched game is what excluded every KANEC run from the first pass.
            skips.append((os.path.basename(run_dir), f"{len(games)} log sections against {len(records)} records (trailing dropped)", len(games) - len(records)))
        openings = openings_by_pair(info)
        opponent, build = opponent_and_build(info)
        for name, scores in zip(records, games):
            winner = replay(os.path.join(run_dir, name))
            # Ours is the record that is not the opponent's. match.ps1 names our game after us and the
            # opponent's game literally "opponent", so the label is read that way instead of looking for one
            # particular name: the engine was renamed from high5 to slap5 and the old test silently read every
            # game of every later run as white, which counted our wins in those runs as losses.
            we_are_black = not name.endswith("_opponent_first.psq")
            colour = "black" if we_are_black else "white"
            ours = 0 if we_are_black else 1
            pair = int(name[1:3]) if name[1:3].isdigit() else -1
            opening = openings[pair] if 0 <= pair < len(openings) else pair
            played[(colour, opponent, opening)] += 1
            if winner is None:
                continue  # no five in the record, or a game that was never finished
            played_agg[(colour, opening)] += 1
            if winner == ours:
                won_agg[(colour, opening)] += 1
                continue  # we won this one
            losses_by_colour[colour] += 1
            lost[(colour, opponent, opening)] += 1
            lost_agg[(colour, opening)] += 1
            if len(scores) < 3:
                skipped += 1
                continue
            # Mate scores (near +-30000) are the end of the game, not a turning point: the search has already
            # seen the finish. The turning point is the first normal-valued drop the score never recovers
            # from - the move that gave the game away - so a fall that comes back is noise.
            normal = [s for s in scores if abs(s) < 29000]
            collapse = None
            for i in range(1, len(normal)):
                if normal[i - 1] - normal[i] > threshold and max(normal[i:]) <= normal[i] + threshold:
                    collapse = i
                    break
            rows.append((run_dir, name, "black" if we_are_black else "white", collapse,
                         len(scores), scores[:min(len(scores), 8)]))

    # A pair index only identifies a position through the openings file, so a corpus that mixes lists cannot
    # have per-opening records pooled from it. Which list each run used is a property of the corpus rather
    # than of the games that could be analysed, so it is reported even when nothing else can be.
    print("opening lists in this corpus (from run-info.txt's openings hash):")
    for digest, runs in opening_lists.most_common():
        note = "" if digest != "?" else "  (run predates the field, so its list is unknown)"
        print(f"  {digest}  {runs} runs{note}")
    known = [d for d in opening_lists if d != "?"]
    if not known:
        print("  no run here records its opening list, so the per-opening tables below cannot be checked against")
        print("           the list they index.")
    if len(known) > 1 or (known and opening_lists.get("?", 0)):
        print("  WARNING: the per-opening tables below pool indices from different or unknown lists, so an")
        print("           index does not name one position. Split the corpus by the hash before trusting them.")
    print()

    if not rows:
        print(f"no lost games with a usable log under {logs_dir} (skipped {skipped})")
        return

    print(f"lost games analysed: {len(rows)} (skipped {skipped})  threshold {threshold}")
    for run_dir, name, colour, collapse, moves, head in sorted(rows, key=lambda r: (r[2], r[3] or 999)):
        where = f"move {collapse} of {moves}" if collapse is not None else f"no collapse of {threshold} in {moves} moves"
        print(f"  {colour:<5} {where:<28} {os.path.basename(run_dir)}/{name}")
        print(f"        first scores: {head}")

    by_colour = Counter(r[2] for r in rows if r[3] is not None)
    buckets = Counter()
    for _, _, _, collapse, moves, _ in rows:
        if collapse is None:
            buckets["no collapse"] += 1
            continue
        share = collapse / max(1, moves)
        buckets["first third" if share < 1 / 3 else "middle third" if share < 2 / 3 else "last third"] += 1
    print()
    print(f"collapsed in: {dict(buckets)}   by colour (where a collapse exists): {dict(by_colour)}")
    print("share of each colour's losses that are one irreversible collapse:")
    for colour in ("black", "white"):
        total = losses_by_colour[colour]
        with_collapse = by_colour[colour]
        share = with_collapse / total if total else 0.0
        print(f"  as {colour:<5} {with_collapse:>4} of {total:>4} losses ({share:.0%})")

    # Openings are the cheapest thing to fix if they are the problem: a pair index in the record name maps to
    # an opening through run-info.txt's opening list, so a bad opening is a repertoire decision, not a search
    # or evaluation one. Denominators are games played as that colour on that opening, ties included.
    print()

    for colour in ("white", "black"):
        print()
        print(f"worst openings as {colour}, per opponent (6 or more games as {colour}):")
        for opponent in sorted({o for (c, o, _) in played if c == colour}):
            ranked = sorted(
                ((op, lost[(colour, opponent, op)], played[(colour, opponent, op)])
                 for (c, o, op) in played
                 if c == colour and o == opponent and played[(colour, opponent, op)] >= 6),
                key=lambda r: -r[1] / r[2],
            )[:6]
            if ranked:
                body = ", ".join(f"op{op} {l}/{n}" for op, l, n in ranked)
                print(f"  vs {opponent:<26} {body}")

    # The decisive-opening test: an opening we never win as white and always win as black is not a problem
    # with our play, it is a first-player win - and in a batch pool those games are decided by the opening
    # list rather than by the engine.
    print()
    print("the openings we lose most as white, seen from both colours (8+ games as white):")
    worst = sorted(
        ((op, lost_agg[("white", op)], played_agg[("white", op)]) for (c, op) in played_agg
         if c == "white" and played_agg[("white", op)] >= 8),
        key=lambda r: -r[1] / r[2],
    )[:14]
    for op, _, _ in worst:
        w, l, n = won_agg[("white", op)], lost_agg[("white", op)], played_agg[("white", op)]
        bw, bl, bn = won_agg[("black", op)], lost_agg[("black", op)], played_agg[("black", op)]
        print(f"  op{op:>2}   as white {w:>3}W-{l:>3}L-{n - w - l:>2}D of {n:>3}    as black {bw:>3}W-{bl:>3}L-{bn - bw - bl:>2}D of {bn:>3}")

    # What did not parse matters as much as what did: if the analysed slice is one arm, one opponent or one
    # period, the numbers above describe that slice rather than the engine.
    print()
    print(f"skipped runs, worst first ({len(skips)} runs, {sum(s[2] for s in skips)} games):")
    for run, reason, count in sorted(skips, key=lambda s: -s[2])[:15]:
        print(f"  {count:>3} games  {reason:<46} {run}")


if __name__ == "__main__":
    main()
