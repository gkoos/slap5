# high5

A freestyle gomoku engine in Rust that speaks the Piskvork/Gomocup protocol. It plays with iterative-deepening alpha-beta; forced-move handling is not written yet (see `docs/algorithm.md`).

## Build and test

```
cargo build --release
cargo test
cargo bench
```

On x86-64 the build uses `target-cpu=native` (see `.cargo/config.toml`), which selects the AVX2 board. Other targets use the scalar board.

## Play

The executable is `target/release/pbrain-high5.exe`. In Piskvork, open Players > Settings (F4) and select it. Piskvork must be set to a 15×15 board; the engine rejects other sizes.

Piskvork keeps its board size in the registry (`height` and `sirka`, the width). To set 15×15 from PowerShell:

```
$k = 'HKCU:\Software\Petr Lastovicka\piskvorky'
New-Item -Path $k -Force | Out-Null
Set-ItemProperty $k -Name height -Value 15 -Type DWord
Set-ItemProperty $k -Name sirka -Value 15 -Type DWord
```

Delete the key to restore the default (20×20). Batch mode (`piskvork.exe -p <engine1> <engine2>`) has no board-size option, so it uses this setting too.

## Matches

```
scripts/match.ps1 -Opponent <engine.exe> [-Pairs 10] [-TimeTurnMs 1000]
```

Plays each of Piskvork's openings twice, once with each engine moving first, prints a win/loss/draw tally and writes the game records and pipe logs under `logs/`. In the same registry key, set `autoBegin` to 1 (otherwise Piskvork ignores openings in batch mode) and `openingRandomShift1` to 0 (so both games of a pair get the same opening). The script kills every running `pbrain-*` process after each game, so run one match at a time.

## Position suite

```
cargo run --release --example suite [-- <ms per position>]
```

Runs the engine on 21 positions taken from games it lost to Tito and PentaZen (`data/last_chance.txt`) and counts how often it picks a move that avoids a forced loss. It is a quick check (about 25 s) before spending an hour on matches. "Saving" means a forced-win prover found no forced win within its bound (at most 4 voluntary threat moves); it is not a proof of safety, and the prover is not in this repository. Baseline: 2/21 at 1000 ms.

## Layout

- `src/bitboard.rs`, `src/avx.rs`: board representation
- `src/zobrist.rs`: hash keys
- `src/threats.rs`: threat masks
- `src/eval.rs`: static evaluation
- `src/search.rs`: the search
- `src/game.rs`, `src/protocol.rs`, `src/main.rs`: game state and Piskvork protocol
- `scripts/match.ps1`: plays matches against other engines
- `examples/suite.rs`, `data/last_chance.txt`: position suite
- `docs/algorithm.md`: the algorithm and what is implemented
- `docs/design-decisions.md`: why things are the way they are
