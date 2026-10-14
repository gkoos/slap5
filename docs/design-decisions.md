# Design decisions

## Scope
- **Freestyle, 15×15 only.** `START` with any other size is answered `ERROR`. Piskvork defaults to 20×20, so its saved board size must be set to 15 (see README).
- **WebAssembly is a secondary target**, native x86-64 is primary.

## Board
- **Padded linear bitboard, one per player.** Cell (row, col) is bit `row*16 + col`; column 15 is always empty, so lines never wrap across rows and no edge masks are needed. The four directions are shifts of 1, 16, 15 and 17.
- **256-bit AVX2 words when available, `[u64; 4]` otherwise.** Selected at compile time (`Words` in `src/lib.rs`). On x86-64, `.cargo/config.toml` sets `target-cpu=native`; without AVX2 the scalar words are used, and they are the reference in tests and the fallback for WebAssembly.
- **Measured** (synthetic positions, one machine, one run): AVX2 was about 3× faster than `[u64; 4]` for win detection and about 4× for a whole-board five-completion mask; make/unmake was slightly slower (86 ns vs 53 ns for 60 operations).
- **Rejected:** `[u128; 2]` (slower than `[u64; 4]` on every operation) and four per-direction line boards (about 1.9× slower than AVX2 on completion masks).

## Threats
- **Masks are computed on demand from the bitboards**, not kept incrementally per cell as PentaZen and Rapfi do. Costs per player per position (30 stones, AVX2): five 54 ns, open four 59 ns, open three 160 ns, four 167 ns. Whether incremental updates would be cheaper is undecided until the search has a real node rate.
- **Open-four defence cells are a superset** of the real defences, which is safe (extra candidates, never missing ones).
- **A double three or four-three is not a rule-based win.** It depends on the opponent's replies, so the search decides.

## Search
- **The Zobrist hash lives in `Board`**, updated by `make` and `unmake` with one XOR, so the search does not need `Game`. Keys are generated at compile time from a fixed seed (splitmix64), so hashes are reproducible between runs. The hash covers stones only; the search adds the side to move.
- **An interrupted iteration is never used.** The move always comes from a completed iteration (PentaZen uses partial results, Rapfi does not).
- **PVS was added after plain alpha-beta, mainly to separate full-window from null-window nodes.** On the suite (1 s per position) it changed mean depth from 5.67 to 5.76 and total nodes by under 1%. 90% of cutoffs come from the first move tried, so later moves are rarely reached. The null-window nodes then use the tighter move-count limit and one more reduction: mean depth 5.76 to 5.86, nodes down 4%. Both were small because about 73% of all nodes were in the leaf search (limit 3 at the time), which neither touches. The tighter limit was later tied to a four or open three on the board instead of to the null window. The limit formulas are PentaZen's as reported by a source read I did not check line by line.
- **Ordering scores every move from the threat masks** (both sides' fives, open fours, fours and open threes); a move with no score is "quiet" and is the only kind pruning and reductions touch. On the suite, 81% of candidates at prunable nodes are quiet.
- **Transposition table: 32 MB, one slot per hash.** A deeper entry from the same search is not overwritten by a shallower one. A result from a deeper search can answer a shallower request, so with the table on the root score can differ from fixed-depth minimax; tests compare against minimax with the table off.
- **Search tests.** The pruned search must equal plain minimax (same rules for decided positions, no ordering or table) at depth 2 and 3 on random positions. The decided-position rules are checked separately against minimax that ends only on a five or a full board. I checked that these tests fail for deliberate pruning bugs (searching only the first move; cutting off 150 points early); a cut-off 30 points early was not detected, because evaluations are multiples of 100.
- **Decided positions and the block-only reply to a four come from PentaZen and Rapfi** (`check_wld`, `quickWinCheck`). On the suite, nodes with a four or open three on the board that were not forced made 74% of all child nodes, searching 58-75 of about 85 candidates; these rules removed them. Depth in games did not change (about 7.3 at 1 s). Building the shared masks once per node instead of up to three times gained only about 6% in nodes per second, so the cause is probably that most removed children were instant losses that cost little (not measured).
- **Switches and counters live in two structs.** `Config` holds the switches and limits (one `Default`, one `apply` for command-line switches); `Stats` holds the counters and prints itself. A switch stays only while it is needed: for tests (an exact search to compare with minimax) or for a feature still being judged. Once a feature is measured and kept, its switch is removed, so the search does not branch on settled choices. PVS and move ordering have no switch for this reason.
- **Only the reply to an open three is a free ply.** Attacking moves cost depth. A free reply to a four was tried first; with the leaf search on it added nothing (suite: 17/21 with and without it, with free threes off) and cost about 0.8 plies of depth, so it was removed. PentaZen and Rapfi do free the reply to a four; neither frees the reply to an open three.
- **The free reply to an open three is chosen from the loss analysis**, not from theory: the opponent's wins in the lost games were chains of 2-4 threats with forced replies, 7-11 plies long. On the position suite (1 s per position) switching it off cost 3 of 21 positions (17/21 against 20/21) and about 2 plies of depth (8.0 against 5.7 mean depth reached). The suite and the prover that built it use the same kind of reasoning, so this overstates real strength; matches are the test.
- **A forced node past the free-ply limit evaluates at depth 0.** Letting it keep expanding made chains unbounded (a bug found while testing).
- **Leaf search for fours, limited to 1 voluntary four.** Both PentaZen and Rapfi run a forced-sequence search at depth 0. With a limit of 4 or more, iterations became so slow that some suite positions finished only depth 1. On the suite (1 s per position) the limits 3, 2, 1 and off all scored 19/21, at mean depths 5.86, 6.48, 6.95 and 7.48, so the suite cannot show its benefit. Against Tito (20 games, 1 s per move) limit 3 and off each scored 3W 17L and limit 1 scored 6W 14L; 20 games only show large differences, so 1 is a choice from one match.
- **A leaf search for fours is included**, as in both engines, so a forcing sequence beyond the search depth is not hidden by the static evaluation.
- **Free-ply cap is measured from the root** and the time check is the real bound. Results found under a cap are then slightly path-dependent in the transposition table; this is accepted and to be measured.

## Protocol
- **Executable name `pbrain-high5`**: Piskvork requires the `pbrain-` prefix.
- **The engine is player 0, the opponent player 1**, whatever colour each plays. `BOARD` labels (1 own, 2 opponent) map to this directly.
- **Opponent moves that are out of range or on an occupied cell are answered `ERROR`** and the engine keeps running.
- **`INFO` is accepted and ignored for now.**

## Testing and evidence
- Bit-parallel code is checked against a separate naive scan over a plain grid on thousands of random positions, for both word types.
- Tests I wrote only show that code matches its stated definitions. Strength is judged by matches against Tito, PentaZen and Rapfi at equal time per move.
- The sources and binaries of those engines are kept in `external/` (git-ignored). Tito is closed source.
