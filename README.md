# high5

A freestyle gomoku engine in Rust that speaks the Piskvork/Gomocup protocol. It plays with iterative-deepening principal variation search (see `docs/algorithm.md` for what is implemented).

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
scripts/match.ps1 -Opponent <engine.exe> [-Pairs 10] [-TimeTurnMs 1000] [-OpeningOffset 0] [-TimeMatchS 600] [-Engine <path>]
```

Plays each of Piskvork's openings twice, once with each engine moving first, prints a win/loss/draw/truncated/error tally and writes the game records, pipe logs and a `run-info.txt` (parameters, opening list, engine hash and repository revision) under `logs/`. In the same registry key, set `autoBegin` to 1 (otherwise Piskvork ignores openings in batch mode) and `openingRandomShift1` to 0 (so both games of a pair get the same opening). The script kills every running `pbrain-*` process after each game, so run one match at a time. `-OpeningOffset` selects held-out openings; `-Engine` compares a second build (for an A/B, build the old revision in a `git worktree`). A game stopped by the match clock is reported as `truncated`, not as an error. Openings used for a big match are recorded and not used again for tuning. The bundled `external\piskvork\bin\pbrain-pela.exe` is a useful second opponent because games against it last long enough to exercise the search. `docs/design-decisions.md` records how changes are measured.

## Position suite

```
cargo run --release --example suite [-- <ms per position>]
```

Runs the engine on 21 positions taken from games it lost to Tito and PentaZen (`examples/last_chance.txt`) and counts how often it picks a move that avoids a forced loss. It is a quick check (about 25 s) before spending an hour on matches. "Saving" means a forced-win prover found no forced win within its bound (at most 4 voluntary threat moves); it is not a proof of safety, and the prover is not in this repository. Two of the 21 entries (`PentaZen-p00_opponent_first` and `Tito-p00_opponent_first`) are the same position. At 1000 ms per position, one process per configuration, on the current revision (`d968e1b` plus the uncommitted work; `lmr-table` and `iir` were measured after their switches went in, everything else on the build that shipped the futility default, `AE6C719D25AFBE96`): default **21/21** (depths 5 to 8, twice), `no-fours` 20/21 (7 to 11), `no-threes` 16/21 (6 to 12), `no-double` 21/21 (5 to 8), `no-counters` 21/21 (5 to 8), `strict-bounds` 21/21 (5 to 7), `no-selective` 17/21 (3 to 6), `no-three-ext` 16/21 (6 to 12), `razor` 14/21 (5 to 8), `lmr-table` 21/21 (5 to 8), `iir` 21/21 (5 to 8), `cap=8` 18/21, `cap=12` and `cap=24` 21/21. The switches are the ones `Config::apply` accepts: `no-tt`, `no-threes`, `no-three-ext`, `no-fours`, `no-counters`, `no-double`, `razor`, `no-razor`, `futility`, `no-futility`, `strict-bounds`, `no-selective`, `no-limit`, `no-reductions`, `lmr-table`, `no-lmr-table`, `iir`, `no-iir`, and the numeric `cap=N` (free-ply cap), `verify=N` and `verify-ms=N` (mate verification bounds). **Futility is on by default** — the first setting here that won games: 5.75 wins per 20-game block against Tito (23 of 80) and 5.33 against pela (16 of 60) on openings 25–34, against 3.67 per block for the build without it and 5.0 for the pre-work control. Calibration made it worse (milder margin 5.25 and 4.00, harsher 3.00 and 6.00), so the uncalibrated `150 + 120·depth` stands. The shipped default build replayed one of those blocks on the same openings and scored 3W–17L, inside the 3–9 wins per block the measured arm itself produced, so the later mate-verification rework did not change playing strength as far as 20 games can tell. **Razoring is off**: 14/21 on the suite against 21/21, twice each, at the same nominal depths (5 to 8), both with `!forced` alone and with PentaZen's `!tactical` guard; a usable version needs a verification search. `no-threes` removes the narrowed reply set to an open three; `no-three-ext` keeps that set and charges its ply. The two are separate so the depth policy can be measured on its own: on the follow-up audit's four attacking positions `no-three-ext` turns 1/4 proved wins into 4/4 (opening 33 goes from depth 1 at +2586, the whole second spent, to mate in 7 in 463 nodes), the defensive suite scores 16/21 against the default's 21/21 with it, and over 40 full games on openings 25–34 against Tito and pela it scored 8 wins against the default's 7 and the pre-work control's 10 — a real defect with no measurable game value, so it is off by default. The default's 21/21 rests on the forcing rules being free: with a four block costing a ply again (`no-fours`) the suite drops to 20/21, and its nominal depth is higher (7 to 11 against 5 to 8) because each iteration then covers fewer plies. The suite cannot see the attacking half: adding our own open threes to the answer to an open three scored *worse* here (19/21) when it was measured, and nothing in `Config::apply` turns it on, so that figure cannot be re-measured as it stands; the double-threat extension scores the same 21/21. The extension was judged by matches, and there it came out level: on the 10–19 opening block it moved the Tito score from 4W–16L to 7W–13L, but on the fresh 25–34 block the same revision scored 3W–16L–1T against the pre-work control's 5W–15L, so over 40 games each the two revisions are 10W–29L–1T and 9W–31L. The swing did not replicate; the extension is kept as principled and not measurably harmful, not as a gain. Earlier history, kept for context: 18/21 at 1000 ms for the default before the per-direction shapes (depths 5 to 9); 19/21 with a separate depth-0 fours search (depths 6 to 9); 3/21 with everything off (`no-threes no-fours no-selective`, mean depth 6.9), 13/21 with pruning and reductions on (depth 9.6).

The suite is 20 unique tactical positions from earlier losses, labelled by a prover that is not in this repository, so it is a fast regression signal and not a measure of playing strength; strength is judged by matches on held-out openings.

## Layout

- `src/bitboard.rs`, `src/avx.rs`: board representation
- `src/zobrist.rs`: hash keys
- `src/threats.rs`: threat masks
- `src/eval.rs`: static evaluation
- `src/search.rs`: the search
- `src/game.rs`, `src/protocol.rs`, `src/main.rs`: game state and Piskvork protocol
- `scripts/match.ps1`: plays matches against other engines
- `examples/suite.rs`, `examples/last_chance.txt`: position suite
- `docs/algorithm.md`: the algorithm and what is implemented
- `docs/design-decisions.md`: why things are the way they are

