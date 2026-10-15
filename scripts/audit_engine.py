"""Read match evidence; optionally replay early loss positions against local engines.

Usage: python scripts/audit_engine.py [--replay] [--ms 1000]
Writes docs/audit-evidence.json. Does not change engine settings or launch Piskvork.
"""
import argparse
import csv
import json
from pathlib import Path
import queue
import re
import statistics
import subprocess
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
RUN = ROOT / "logs/pbrain-Tito2014_20261014_235818"
DEBUG = re.compile(r"DEBUG depth (\d+) score (-?\d+) nodes (\d+) forced (\d+) leaf (\d+) horizon (\d+) expansions (\d+) cutoffs (\d+) first (\d+) pruned (\d+) time (\d+)ms")
KEYS = "depth score nodes forced leaf horizon expansions cutoffs first pruned ms".split()
XY = re.compile(r"^\d+,\d+$")


def sessions():
    result = []
    for block in (RUN / "pbrain-high5.exe.log").read_text().split("______Process started______")[1:]:
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
            elif m := DEBUG.fullmatch(line):
                pending = dict(zip(KEYS, map(int, m.groups())))
                pending["board"] = list(board)
            elif XY.fullmatch(line) and pending is not None:
                x, y = map(int, line.split(","))
                pending["move"] = [x, y]
                records.append(pending)
                board.append([0, x, y])
                pending = None
        result.append(records)
    return result


def validate_record(path):
    moves = [tuple(map(int, line.split(",")))[:2] for line in path.read_text().splitlines() if re.fullmatch(r"\d+,\d+,\d+", line)]
    grid = {}
    first_win = None
    for n, (x, y) in enumerate(moves):
        assert 1 <= x <= 15 and 1 <= y <= 15 and (x, y) not in grid, path
        side = n % 2
        grid[x, y] = side
        for dx, dy in [(1, 0), (0, 1), (1, 1), (1, -1)]:
            length = 1
            for sign in [-1, 1]:
                k = 1
                while grid.get((x + sign * k * dx, y + sign * k * dy)) == side:
                    length += 1
                    k += 1
            if length >= 5 and first_win is None:
                first_win = {"side": side, "ply": n + 1, "moves": len(moves)}
    return first_win


def query_engine(executable, board, ms):
    proc = subprocess.Popen([str(executable)], cwd=executable.parent, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True,
                            creationflags=subprocess.CREATE_NO_WINDOW)
    lines = queue.Queue()
    def read():
        for line in proc.stdout:
            lines.put(line.rstrip())
        lines.put(None)
    threading.Thread(target=read, daemon=True).start()
    response = []
    started = time.monotonic()
    try:
        payload = ["START 15", "INFO rule 0", "INFO max_memory 83886080", f"INFO timeout_turn {ms}",
                   "INFO timeout_match 300000", "INFO time_left 300000", "BOARD"]
        payload += [f"{x},{y},{p + 1}" for p, x, y in board]
        proc.stdin.write("\n".join(payload + ["DONE"]) + "\n")
        proc.stdin.flush()
        deadline = started + ms / 1000 + 15
        while time.monotonic() < deadline:
            line = lines.get(timeout=max(0.01, deadline - time.monotonic()))
            if line is None:
                break
            response.append(line)
            if XY.fullmatch(line):
                return {"move": list(map(int, line.split(","))), "wall_ms": round((time.monotonic() - started) * 1000), "output": response}
        return {"error": "no move", "output": response}
    except queue.Empty:
        return {"error": "timeout", "output": response}
    finally:
        if proc.poll() is None:
            try:
                proc.stdin.write("END\n")
                proc.stdin.flush()
                proc.wait(timeout=2)
            except (OSError, subprocess.TimeoutExpired):
                proc.kill()
                proc.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--replay", action="store_true")
    parser.add_argument("--ms", type=int, default=1000)
    args = parser.parse_args()
    games = list(csv.DictReader((RUN / "results.csv").open()))
    records = sessions()
    assert len(games) == len(records) == 20
    all_records = [r for game in records for r in game]
    thinking = [r for r in all_records if abs(r["score"]) < 29744 and r["ms"] > 0]
    evidence = {"run": str(RUN.relative_to(ROOT)), "games": [], "summary": {
        "decisions": len(all_records), "nonmate_timed_decisions": len(thinking),
        "mean_depth": statistics.mean(r["depth"] for r in thinking),
        "mean_ms": statistics.mean(r["ms"] for r in thinking),
        "mean_nps": statistics.mean(r["nodes"] * 1000 / r["ms"] for r in thinking),
        "below_600ms": sum(r["ms"] < 600 for r in thinking),
        "leaf_node_fraction": sum(r["leaf"] for r in all_records) / sum(r["nodes"] for r in all_records),
        "first_cutoff_fraction": sum(r["first"] for r in all_records) / sum(r["cutoffs"] for r in all_records),
    }}
    tito = [tuple(map(int, m)) for m in re.findall(r"MESSAGE AB depth=\s*(\d+) node=\s*(\d+) speed=\s*(\d+)n/s", (RUN / "pbrain-Tito2014.exe.log").read_text())]
    evidence["summary"]["tito_reported_searches"] = len(tito)
    evidence["summary"]["tito_mean_reported_depth"] = statistics.mean(r[0] for r in tito)
    evidence["summary"]["tito_mean_reported_nps"] = statistics.mean(r[2] for r in tito)
    evidence["summary"]["identical_paired_openings"] = 0
    for pair in range(10):
        starts = []
        for first in ["high5", "opponent"]:
            text = (RUN / f"p{pair:02}_{first}_first.psq").read_text()
            starts.append([line.split(",")[:2] for line in text.splitlines() if re.fullmatch(r"\d+,\d+,\d+", line)][:3])
        assert starts[0] == starts[1], f"different opening in pair {pair}"
        evidence["summary"]["identical_paired_openings"] += 1
    for row, rs in zip(games, records):
        label = f"p{int(row['Pair']):02}_{'high5' if row['High5First'] == 'True' else 'opponent'}_first"
        terminal = validate_record(RUN / (label + ".psq"))
        assert terminal and terminal["ply"] == terminal["moves"]
        expected_side = 0 if row["ExitCode"] == "1" else 1
        assert terminal["side"] == expected_side
        first_loss = next((i for i, r in enumerate(rs) if r["score"] <= -29744), None)
        item = {"name": label, "outcome": row["Outcome"], "terminal": terminal,
                "first_loss_index": first_loss, "decisions": [{k: v for k, v in r.items() if k != "board"} for r in rs]}
        if first_loss is not None:
            item["first_loss_stones"] = len(rs[first_loss]["board"])
        evidence["games"].append(item)
        if args.replay and label in ["p00_high5_first", "p00_opponent_first", "p02_high5_first", "p04_opponent_first"] and first_loss is not None:
            index = max(0, first_loss - 2)
            item["replay"] = {"decision_index": index, "board": rs[index]["board"], "historical": rs[index], "budget_ms": args.ms, "engines": {}}
            for engine, executable in [
                ("high5", ROOT / "target/release/pbrain-high5.exe"),
                ("pentazen", ROOT / "external/pentazen/bin/pbrain-PentaZen21_15.exe"),
                ("rapfi", ROOT / "external/rapfi/bin/pbrain-rapfi_avx2.exe"),
            ]:
                item["replay"]["engines"][engine] = query_engine(executable, rs[index]["board"], args.ms)
                print(label, engine, json.dumps(item["replay"]["engines"][engine]), flush=True)
    out = ROOT / "docs/audit-evidence.json"
    out.write_text(json.dumps(evidence, indent=2) + "\n")
    print(json.dumps(evidence["summary"], indent=2))
    print("All 20 records end in a legal five matching the recorded winner.")


if __name__ == "__main__":
    main()
