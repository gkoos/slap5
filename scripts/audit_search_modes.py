"""Instrument a disposable search copy; production files are never edited.

python scripts/audit_search_modes.py --mode no-free-three --ms 1000
Modes retain open-three candidate restrictions; only the depth extension changes.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]


def record_output(mode, output):
    records = []
    for line in output.splitlines():
        match = re.match(r"(opening\d+_\w+) side=(\d) static=(-?\d+) move=\((\d+), (\d+)\) score=(-?\d+) depth=(\d+) ms=(\d+) (.*)", line)
        if match:
            name, side, static, x, y, score, depth, ms, tail = match.groups()
            result = dict(name=name, side=int(side), static=int(static), move=[int(x), int(y)],
                          score=int(score), depth=int(depth), ms=int(ms))
            result.update({k: int(v) for k, v in re.findall(r"(\w+) (\d+)", tail)})
            records.append(result)
        elif line.startswith("audit "):
            assert records
            records[-1]["instrumentation"] = {k: int(v) for k, v in re.findall(r"(\w+)=(\d+)", line)}
            if "root_moves=" in line:
                records[-1]["instrumentation"]["root_moves"] = ast.literal_eval(line.split("root_moves=")[1])
    assert len(records) == 4, "instrumentation output did not contain all four positions"
    path = ROOT / "docs/audit-followup-experiments.json"
    evidence = json.loads(path.read_text()) if path.exists() else {"modes": {}}
    evidence["binary_sha256"] = hashlib.sha256((ROOT / "target/release/pbrain-high5.exe").read_bytes()).hexdigest()
    evidence["modes"][mode] = records
    path.write_text(json.dumps(evidence, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--mode", choices=["default", "no-free-three", "positive-three", "fresh-verify"], default="default")
    parser.add_argument("--ms", type=int, default=1000)
    parser.add_argument("flags", nargs="*")
    args = parser.parse_args()
    scratch = ROOT / "logs/audit-followup-search"
    scratch.mkdir(parents=True, exist_ok=True)
    manifest = ('[package]\nname="audit-followup-search"\nversion="0.0.0"\nedition="2024"\n'
                '[dependencies]\nhigh5={path="../.."}\n[[bin]]\nname="audit-followup-search"\npath="main.rs"\n')
    (scratch / "Cargo.toml").write_text(manifest)
    source = (ROOT / "src/search.rs").read_text()
    old = "let free = (three_reply || four_reply) && ply < self.config.free_ply_limit;"
    assert source.count(old) == 1
    expression = {"no-free-three": "four_reply", "positive-three": "(four_reply || (three_reply && depth > 0))"}.get(args.mode, "(three_reply || four_reply)")
    source = source.replace(old, f"let free = {expression} && ply < self.config.free_ply_limit;")
    source = source.replace("pub nodes: u64,", "pub nodes: u64,\n    pub audit_three: u64,\n    pub audit_three_candidates: u64,\n    pub audit_zero_three: u64,\n    pub audit_max_ply: usize,\n    pub audit_tt: u64,\n    pub audit_verifications: u64,\n    pub audit_verify_nodes: u64,\n    pub audit_verify_tt: u64,\n    pub audit_root_moves: [u64; 16],", 1)
    source = source.replace("let root = ply == 0;", "self.stats.audit_max_ply = self.stats.audit_max_ply.max(ply);\n        let root = ply == 0;")
    source = source.replace("let forced = block_four || three_reply;", "if three_reply { self.stats.audit_three += 1; self.stats.audit_zero_three += (depth <= 0) as u64; }\n        let forced = block_four || three_reply;")
    source = source.replace("let scores = Scores::new(&shapes, side, cand);", "if three_reply { self.stats.audit_three_candidates += cand.count_ones() as u64; }\n        let scores = Scores::new(&shapes, side, cand);")
    source = source.replace("searched += 1;", "if root { self.stats.audit_root_moves[depth.clamp(0, 15) as usize] += 1; }\n            searched += 1;")
    source = source.replace("if let Some(s) = settled {\n                return s;", "if let Some(s) = settled {\n                self.stats.audit_tt += 1;\n                return s;")
    source = source.replace("let saved = self.config;", "let saved = self.config;\n        self.stats.audit_verifications += 1;\n        let audit_before = (self.stats.nodes, self.stats.audit_tt);")
    if args.mode == "fresh-verify":
        source = source.replace("self.config.selective = false;", "self.config.selective = false;\n        self.config.use_tt = false;")
    source = source.replace("self.config = saved;", "self.stats.audit_verify_nodes += self.stats.nodes - audit_before.0;\n        self.stats.audit_verify_tt += self.stats.audit_tt - audit_before.1;\n        self.config = saved;")
    source = source.replace("#[cfg(test)]\nmod tests;", "")
    (scratch / "search.rs").write_text(source)
    main_source = (ROOT / "examples/audit_followup.rs").read_text()
    main_source = main_source.replace("use high5::search::{Config, Limits, Searcher};", "pub use high5::{bitboard, threats, zobrist, Words};\nmod search;\nuse search::{Config, Limits, Searcher};")
    main_source = main_source.replace('include_str!("../data/audit_followup.txt")', 'include_str!("../../data/audit_followup.txt")')
    marker = 'result.stats\n        );'
    assert main_source.count(marker) == 1
    main_source = main_source.replace(marker, marker + '\n        let s = result.stats;\n        println!("audit three={} candidates={} zero_three={} max_ply={} tt={} verify_calls={} verify_nodes={} verify_tt={} root_moves={:?}", s.audit_three, s.audit_three_candidates, s.audit_zero_three, s.audit_max_ply, s.audit_tt, s.audit_verifications, s.audit_verify_nodes, s.audit_verify_tt, s.audit_root_moves);')
    (scratch / "main.rs").write_text(main_source)
    print(f"mode={args.mode}", flush=True)
    result = subprocess.run(["cargo", "run", "--release", "--manifest-path", str(scratch / "Cargo.toml"), "--", str(args.ms)] + args.flags,
                            cwd=ROOT, check=True, capture_output=True, text=True)
    print(result.stderr, end="")
    print(result.stdout, end="")
    mode = ("defend-" if "defend" in args.flags else "") + args.mode
    (ROOT / f"logs/audit-followup-{mode}.txt").write_text(result.stdout)
    record_output(mode, result.stdout)


if __name__ == "__main__":
    main()
