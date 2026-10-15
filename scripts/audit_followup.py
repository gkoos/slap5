"""Second-audit evidence: current match logs and an independent continuous-four prover.

The prover returns a forcing certificate or 'not found within limits'; it does not
claim that positions without a certificate are safe. Run from the repository root.
"""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import re
import time

ROOT = Path(__file__).resolve().parents[1]
VALID = sum(1 << (y * 16 + x) for y in range(15) for x in range(15))
WINDOWS = []
for dx, dy in [(1, 0), (0, 1), (1, 1), (1, -1)]:
    for y in range(15):
        for x in range(15):
            if 0 <= x + 4 * dx < 15 and 0 <= y + 4 * dy < 15:
                WINDOWS.append(sum(1 << ((y + k * dy) * 16 + x + k * dx) for k in range(5)))


def cells(bits):
    while bits:
        b = bits & -bits
        yield b.bit_length() - 1
        bits ^= b


def threats(own, other):
    five = four = 0
    for w in WINDOWS:
        if w & other:
            continue
        n = (w & own).bit_count()
        if n == 4:
            five |= w & ~own
        elif n == 3:
            four |= w & ~own
    return five, four


class VCF:
    def __init__(self, max_nodes=10000):
        self.nodes = 0
        self.max_nodes = max_nodes
        self.failed = {}

    def solve(self, own, other, attacks=10):
        self.nodes += 1
        if self.nodes > self.max_nodes:
            raise TimeoutError("VCF node budget")
        wins, fours = threats(own, other)
        if wins:
            return [next(cells(wins))]
        if attacks <= 0 or self.failed.get((own, other), -1) >= attacks:
            return None
        opp_wins, _ = threats(other, own)
        if opp_wins.bit_count() > 1:
            return None
        if opp_wins:
            fours &= opp_wins
        # Look at the direct double threats first; other forcing branches remain complete.
        choices = []
        for move in cells(fours):
            new_own = own | (1 << move)
            follow, _ = threats(new_own, other)
            choices.append((-follow.bit_count(), move, follow))
        for _, move, follow in sorted(choices):
            if follow.bit_count() >= 2:
                ends = list(cells(follow))
                return [move, ends[0], ends[1]]
            if not follow:
                continue
            block = next(cells(follow))
            rest = self.solve(own | (1 << move), other | (1 << block), attacks - 1)
            if rest is not None:
                return [move, block] + rest
        self.failed[own, other] = attacks
        return None


def bits(board, side):
    return sum(1 << (y * 16 + x) for p, x, y in board if p == side)


def validate_certificate(board, line, attacker=1):
    """Check a forcing certificate with a separate grid scanner, not the bit-window prover."""
    grid = {(x, y): side for side, x, y in board}
    assert len(grid) == len(board)

    def makes_five(x, y, side):
        for dx, dy in [(1, 0), (0, 1), (1, 1), (1, -1)]:
            length = 1
            for sign in [-1, 1]:
                k = 1
                while grid.get((x + sign * k * dx, y + sign * k * dy)) == side:
                    length += 1
                    k += 1
            if length >= 5:
                return True
        return False

    def completions(side):
        return {(x, y) for y in range(15) for x in range(15)
                if (x, y) not in grid and makes_five(x, y, side)}

    assert not any(makes_five(x, y, side) for (x, y), side in grid.items())
    for i, move in enumerate(line):
        x, y = move
        side = attacker if i % 2 == 0 else 1 - attacker
        assert 0 <= x < 15 and 0 <= y < 15 and (x, y) not in grid
        if side != attacker:
            assert not completions(side), "defender could win instead of blocking"
            assert (x, y) in completions(attacker), "reply is not a forced block"
        grid[x, y] = side
        if makes_five(x, y, side):
            assert side == attacker and i == len(line) - 1
            return True
        if side == attacker:
            assert completions(attacker), "attacking move does not force a reply"
    raise AssertionError("certificate does not finish with a five")


def sessions(run):
    result = []
    # The engine log is named after the binary, and that differs between arms (`pbrain-high5.exe`,
    # `pbrain-high5-noext.exe`), so take the run's own high5 log rather than a fixed name.
    logs = sorted(run.glob("pbrain-high5*.exe.log"))
    assert len(logs) == 1, f"expected one high5 log in {run}, found {[p.name for p in logs]}"
    for block in logs[0].read_text().split("______Process started______")[1:]:
        board, records, collecting, pending = [], [], False, None
        for line in block.splitlines():
            if line == "BOARD":
                board, collecting = [], True
            elif collecting and line == "DONE":
                collecting = False
            elif collecting and re.fullmatch(r"\d+,\d+,[12]", line):
                x, y, flag = map(int, line.split(","))
                board.append([flag - 1, x, y])
            elif line.startswith("TURN "):
                x, y = map(int, line[5:].split(","))
                board.append([1, x, y])
            elif line.startswith("DEBUG depth "):
                pending = {k: int(v) for k, v in re.findall(r"(\w+) (-?\d+)", line)}
                pending["board"] = list(board)
            elif re.fullmatch(r"\d+,\d+", line) and pending is not None:
                x, y = map(int, line.split(","))
                pending["move"] = [x, y]
                records.append(pending)
                board.append([0, x, y])
                pending = None
        result.append(records)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--run", default="logs/pbrain-Tito2014_20261015_140104")
    parser.add_argument("--nodes", type=int, default=3000)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--ms", type=int, default=1000)
    args = parser.parse_args()
    run = ROOT / args.run
    rows = list(csv.DictReader((run / "results.csv").open()))
    games = sessions(run)
    assert len(rows) == len(games)
    binary = ROOT / "target/release/pbrain-high5.exe"
    evidence = {"run": args.run, "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                "search_source_sha256": hashlib.sha256((ROOT / "src/search.rs").read_bytes()).hexdigest(),
                "cases": []}
    for row, rs in zip(rows, games):
        if row["Outcome"] != "loss":
            continue
        first_loss = next((i for i, r in enumerate(rs) if r["score"] <= -29744), len(rs) - 1)
        # Opponent to move immediately after a high5 move, before high5 sees mate.
        for index in range(max(0, first_loss - 4), first_loss):
            r = rs[index]
            board = r["board"] + [[0] + r["move"]]
            prover = VCF(args.nodes)
            started = time.monotonic()
            try:
                line = prover.solve(bits(board, 1), bits(board, 0))
            except TimeoutError:
                continue
            if line is not None:
                item = {"pair": int(row["Pair"]), "opening": int(row["Opening"]),
                        "high5_first": row["High5First"], "decision": index,
                        "historical_score": r["score"], "historical_depth": r["depth"],
                        "first_loss_decision": first_loss, "board": board,
                        "attacker": 1, "line": [[i % 16, i // 16] for i in line],
                        "vcf_nodes": prover.nodes, "vcf_ms": round((time.monotonic() - started) * 1000)}
                assert validate_certificate(board, item["line"])
                item["certificate_validated"] = True
                if args.replay:
                    from audit_engine import query_engine
                    replay_board = [[1 - side, x, y] for side, x, y in board]
                    item["replay"] = {}
                    for name, path in [
                        ("high5", "target/release/pbrain-high5.exe"),
                        ("pentazen", "external/pentazen/bin/pbrain-PentaZen21_15.exe"),
                        ("rapfi", "external/rapfi/bin/pbrain-rapfi_avx2.exe"),
                    ]:
                        item["replay"][name] = query_engine(ROOT / path, replay_board, args.ms)
                evidence["cases"].append(item)
                print(json.dumps({k: v for k, v in item.items() if k not in ["board", "replay"]}), flush=True)
                break
    out = ROOT / "docs/audit-followup-cases.json"
    if not evidence["cases"]:
        print(f"no certificate in {args.run}; the corpus and the case file were left as they are")
        return
    out.write_text(json.dumps(evidence, indent=2) + "\n")
    position_lines = ["# Attacking positions from the second audit; original players high5=0, Tito=1."]
    for item in evidence["cases"]:
        name = f"opening{item['opening']}_{'first' if item['high5_first'] == 'True' else 'second'}"
        position_lines.append(name + " us=1 stones=" + " ".join(f"{p}:{x},{y}" for p, x, y in item["board"]))
    (ROOT / "data/audit_followup.txt").write_text("\n".join(position_lines) + "\n")
    print(f"{len(evidence['cases'])} certificates written to {out}")


if __name__ == "__main__":
    main()
