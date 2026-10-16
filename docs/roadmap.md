# Improvement queue

The order the evidence supports as of 16 October 2026.

The only change in this work that won games was **futility pruning**, which deletes subtrees. Every change that only shaved depth or bought speed was level on games: the two-dimensional reduction table (`lmr-table`, 25W-71L over 100 games against Tito), IIR (`iir`, 30W over 93 games), and the three local optimizations (deferred evaluation, the read-only bit iterator, the bulk discard of the pruned tail), which together bought 5-7% more search per second and changed no result. So deletion-class work first; speed-class work is expected to be neutral unless it buys a whole ply.

Where we stand, for calibration: roughly **1450-1550** in freestyle15 terms. The two usable near-equal opponents are **KANEC** (1478, 816 rated games, opposition averaging 1328) and **PELA 2023** (1549, 1320 games, opposition 1514). Use `external/pela/pbrain-pela.exe` for the latter: that is the build the rating refers to. Piskvork's bundled `external/piskvork/bin/pbrain-pela.exe` is a different, much smaller build of the same version string and must not be used for measurement. Tito (1764) beats us and Rapfi (2716) sweeps us 20-0.

## Measurement rules

Unchanged from the earlier work.

- A mechanism lands behind its own switch, default off, with the default path untouched until a pool says otherwise.
- Equivalence gates: the 62 tests, the position suite (21/21) twice, and a fixed-depth or fixed-position comparison showing identical moves, scores, depths and `Stats` where the change is supposed to be an equivalence.
- Strength gate: 100-game pools (five 20-game blocks on held-out openings 25-34) against KANEC and pela, tallied per block, with truncated and error games counted separately.
- Twenty games cannot see a 20-Elo effect. The block-to-block swing of one binary is 3-9 wins out of 20, and the standard error of a 100-game difference is about six wins.
- No item is adopted on a Rapfi result. At 0-20 there is no resolution to gain there.

## 1. In-node VCF (deletion class)

A bounded forced-win search for continuous fours, run inside a node, returning a fail-high cutoff or nothing.

Why first: it attacks the loss mode we can reproduce. Against Rapfi we are mate-proved while at depth 1 with 112 nodes spent, where Rapfi reports depth 26 and finds its mate in 3 in 50 nodes; against Tito the recorded losses arrive in about twenty moves per side, which is forced play. Half the nodes in a probe position were `forced` nodes, so this is work we already do - the general search does it with a window, a table and move ordering, a dedicated bounded search does it with neither.

Cost and risk: a node budget per call, and wasted time in quiet positions. That risk is removable by construction: gate the call on a mask already computed at every node (the side to move has a four, or an open three) plus a depth cap, and the quiet-position cost is zero rather than small.

Work: the bounded fours-and-open-threes search, a per-turn node budget, the guard, a `Stats` counter, a switch. Verify with the suite, then a corpus of the Rapfi endings from `logs/pbrain-rapfi_avx2_20261016_020338/*.psq` as a deterministic does-it-now-prove-the-mate check, then the 100-game pools.

## 2. Null-move pruning (deletion class)

Rapfi has it (`nullMoveReduction`, with a per-rule bias); we have none. It deletes subtrees at quiet nodes: pass the move, search shallower, cut if the opponent still cannot profit. In gomoku the guard is what makes it safe - the opponent must have no four or five threat, and the position must not be a forced chain, or the pass is illegal in effect. Those masks already exist here (`opp_five`, `opp_open4`).

Why second: it is the same class as futility, which is the only thing that has worked, and it fires where futility does.

Cost and risk: a switch, a reduction constant and a guard set. The failure mode, if a guard is wrong, is missing the opponent's win - which the suite and the tactical corpus are built to catch.

## 3. Evaluation tuning

Our evaluation is twelve hand-set weights over per-direction shape classes (class values 3000/1500/300/100/25/8, plus two weight rows for ordering), with no positional term, no centrality, no tempo term, and own/opponent weights separated by hand. It has never been fitted to anything. This is the cheapest strength in the tree: no new code path, no new data structure, no new switch beyond a way to load alternative weights.

Three steps, increasing in cost: **(a)** fit the existing twelve numbers against a fixed opponent (KANEC) by hill-climbing or SPSA; **(b)** add the missing terms - centrality, tempo, and the blocked-three/two weights that currently do double duty as the development signal; **(c)** consider richer features only after that.

Why not later: it is also the prerequisite for any learned evaluation. A net trained on labels from a fifteen-hundred engine with untuned features would mostly learn to reproduce twelve numbers.

## 4. Ordering and candidates

Two concrete defects, both measured in the earlier audit and both untouched:

- **The searcher never sees the real preceding moves of the game.** Recent-move proximity uses only the hypothetical search path, so at the root it does not know what the opponent actually just played. The protocol already hands us those moves; nothing consumes them.
- **The candidate radius is a fixed 2 cells** from any stone. PentaZen widens its range near the root while fewer than five stones are on the board, so our opening play starts narrower than theirs.

Why here: ordering and candidate changes are the cheapest way to turn a search that reaches depth 5-8 into one that reaches deeper on the same budget, and both defects are in the part of the engine the references differ from most. They change which moves exist or in what order they are tried, so they need the full gates - identical fixed-depth results where they are meant to be neutral, and a pool where they are not.

## 5. Transposition-table policy

One slot per hash, no replacement policy, no clustering. A deeper entry is never replaced by a shallower one - which the earlier audit measured as a cost, since a same-age entry belonging to another position is kept whole. Both references cluster their tables.

A clustered bucket - several entries, replace the shallowest or the oldest - is contained and testable (identical fixed-depth results under a table-on/table-off comparison harness). Expected effect is modest and mostly in node rate, so treat it as speed: adopt only if it buys enough to show up as depth.

## 6. Time management

The deadline is the full move budget (`self.deadline = start + limits.time`); there is no half-budget reservation in the current code, and the older claim that there was one was written against an earlier revision. What is still open is small but free: does the root loop start an iteration it cannot finish, and is the budget spent evenly across a game? Answer it with per-move elapsed times and `Stats`, not with a rating test. Anything more elaborate - extending on score drop or move instability - is a match question, and at 1 s per move it is likely to be noise.

## Rejected or deferred

- **The per-node double-threat mask**: measured at -4.5% in its own audit and +5.0% when re-measured on the current tree. A sign flip between runs is not a basis for a change.
- **Compact `Shapes`**: the memory saving is real and confirmed by the layout check (3072 -> 1920 bytes), the time delta is +2.7%, i.e. slower. Memory only, and this engine is not memory-bound.
- **Narrowing `Scores::shape` to `i16`**: the bound is 1800, so it fits, but it was never benchmarked.
- **NNUE or any learned evaluation**: deferred as its own project, deliberately not in this document. It needs a data pipeline (self-play at fixed depth, a few million positions), a trainer, an exporter, quantised inference, and an incremental accumulator hooked into `make`/`unmake` - and before any of that, a test that the incremental accumulator equals a full recompute at every ply, which is the classic silent failure of that design. It also needs what items 3 and 4 produce: a tuned classical evaluation as the baseline, and labels from an engine as strong as we can currently make it.

## How each item is measured

| item | equivalence gate | strength gate |
|---|---|---|
| in-node VCF | suite 21/21 twice; the Rapfi-endings corpus proves or does not prove | 100-game pools against KANEC and pela |
| null-move pruning | suite 21/21 twice; no mate missed on the corpus | 100-game pools against both |
| evaluation tuning | fixed-depth scores identical where the weights are unchanged | a KANEC pool first, per the equal-opponent rule |
| ordering and candidates | identical fixed-depth moves, scores and depths where neutral | 100-game pools against both |
| transposition table | identical fixed-depth results with the table on and off | only if the depth gain reaches a ply |
| time management | per-move elapsed time and `Stats`, not a match | none - it is a probe |

