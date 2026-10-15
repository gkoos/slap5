# Algorithm

Freestyle gomoku (15×15, five or more in a row wins), played over the Piskvork protocol. Status is marked per section.

## Search (baseline implemented)

`src/search.rs`: iterative-deepening principal variation search (negamax). Each iteration searches one ply deeper than the last. An iteration interrupted by the deadline is discarded entirely; the move comes from the last completed iteration, never from a partial one. Depth 1 is never interrupted, so a move always exists.

- Transposition table: 2^21 entries of 16 bytes (32 MB), keyed by the Zobrist hash plus the side to move; stores depth, bound, score and best move. Depth-preferred replacement within one search, anything is replaced once an entry is from an earlier search.
- Move ordering: table move first, then a score from the shape classes of both sides. `src/threats.rs` classifies, per direction and for both players, the shape a stone would create: five, open four, four, open three, blocked three, two. A cell belongs to the strongest class it makes in one direction, and the class is counted in every direction that shows it, so a stone making two fours scores twice. Weights per class, summed over directions: 20000/10000/2000/1000/250/40 for the side to move and 20000/5000/800/500/200/30 for the opponent (blocking), plus 4000/3000 for the two killer moves of the ply and the history heuristic. The blocked-three and two part is quartered when the cell is more than 4 cells from each of the last four moves of the current line and halved when it is more than 4 from each of the last two; the forcing classes are never discounted. A move with none of the four forcing classes in any direction is "quiet".
- Quiet moves: a move that makes or blocks no five, four or open three and is not a killer. At nodes that are not the root and not forced, moves past a move-count limit are skipped (at ply 1 only quiet ones): `round(15*sqrt(d) - 12)` clamped to 3..32, or `round(5*sqrt(d) - 2)` clamped to 2..8 when a four or open three is on the board. Nothing is skipped while every move searched so far loses.
- Late move reductions: from the fourth move, quiet moves at depth 3 or more are searched 1 ply shallower (2 from the ninth move at depth 6 or more, plus 1 at null-window nodes), and again at full depth if they beat alpha. `selective` switches the pruning and the reductions off together, which makes the search exact; `no-limit` and `no-reductions` switch them off one at a time, for measuring each on its own.
- Principal variation search: the first move gets the full window; later moves get a null window (alpha, alpha+1) and are searched again with the full window only if they land strictly between alpha and beta.
- Candidates: empty cells within 2 of an existing stone. On an empty board the engine plays the centre.
- Decided positions (`Threats::decided`, at every node except the root, and in the leaf search): a full board is a draw; a side that can make five wins next move; two fives threatened against the side to move lose in 2 plies (only one can be blocked); an open four for the side to move, with no five threatened against it, wins in 3 plies. These are checked before the depth test; at depth 0 the leaf search or the static evaluation is returned. The five-cell and open-four masks for both sides are built once per node and shared by this check, the forced-move choice and move ordering.
- Scores: a win at ply p scores `30000 - p`, so the shortest win is preferred. Table entries store mate scores relative to the node.
- Time: the deadline is checked every 1024 nodes, and no new iteration starts once half the budget is used. The budget is `INFO timeout_turn` minus 50 ms, at most a tenth of `time_left` (a placeholder for proper time management).

## Forced replies (implemented)

When the side to move is forced, its candidates are narrowed; only the reply to an open three is searched without reducing the depth counter:

| Case | Side to move | Candidates | Depth |
|---|---|---|---|
| opponent has a four | must block | the blocking cell | costs one ply |
| opponent has an open three | must stop it or counter | blocking cells plus own fours | free (`free_three_replies`) |
| none of the above | | ordinary candidates; fours and threes ordered first | costs one ply |

A reply to a four is not free: the leaf search follows fours, so a free block would only repeat it. A move that creates a threat costs a ply like any other. A double three or four-three is not treated as a win by rule; the search proves or refutes it. Forced nodes are expanded even when the remaining depth is 0. Free plies stop at ply 64 from the root, after which a forced node costs depth like any other, and the deadline check applies inside chains. **The cap is the switch that decides how far the chains run, and it is measurable (`cap=N`):** at 1 s on the five positions of `docs/engine-audit-followup.md`, nominal depth is 5–8 at cap 12 and 2–5 at cap 64, with the same one second, and the mate against us on opening 26 first is found at cap 12 (107 ms) and missed at cap 64. The defensive suite reads 21/21 at cap 12 (twice, stable), 20/21 at cap 24 and 64, and 19/21 at cap 8. `Stats::max_ply` is the deepest ply any node reached, free replies included, so the nominal depth and the real reach can be read apart; in the 25–34 match the default averages **nominal depth 5.2 and maxply 55.0** against Tito's reported 11.3. Games did not follow the depth: cap 12 scored 4W–16L against Tito on openings 25–34, inside the 3–5 wins the same cap-64 build scored on that block in two runs. The first iteration (depth 1) is plain, so it stays cheap and always completes. `Stats::forced_nodes` counts the forced nodes.

`free_three_replies` defaults to on, and the depth cost of those replies is a separate setting: `extend_three_replies` (switch `no-three-ext`), also on by default. The reply set stays when the switch is off and only the free ply goes. **Measured on the follow-up audit's four attacking positions** (`cargo run --release --example audit_followup -- 1000 [no-three-ext]`): the default proves 1 of 4 and `no-three-ext` proves 4 of 4 — opening 33 goes from depth 1 at +2586 having eaten the whole second to mate in 7 at depth 2 in **463 nodes**, opening 28 second from +2466 to mate in 9, opening 30 second from +1441 to mate in 13, and opening 28 first from 106 ms to 9 ms for the same mate in 11. **On the 21-position defensive suite the same switch scores 16/21 against 20/21** (depths 6–11 against 4–7). **And on full games it is a wash:** openings 25–34 against Tito and pela, 40 games per engine, `no-three-ext` scored **8 wins** against the default build's **7** and the pre-work control's **10** (Tito 3W–17L, pela 5W–14L–1T), with 5 of the 20 Tito games decided differently by the two builds — so the search defect it fixes is real and the game-level value is nothing measurable. The switch stays **off**: `extend_three_replies` defaults to on. The blocking cells for an open three are a superset of the real defences (see `open_four_block_cells`), which only adds candidates.

## Forcing play at the horizon (implemented)

There is no separate depth-0 search. At remaining depth 0 the node evaluates, unless its reply is forced: a check on the opponent (four or more five-threats against us, or an open three) restricts the moves to the defences and keeps the depth, so the same search follows four and three chains itself, bounded by `free_ply_limit` from the root.
- `Threats::decided` handles the cases that need no search at all: our five next move, two five-threats against us, and our own open four.
- A block of a four costs nothing (`free_four_replies`, switch `no-fours` turns it off) and the reply to an open three costs nothing (`free_three_replies`, switch `no-threes`), so a four and its block cost one ply together and a three and its answer cost one ply — as in PentaZen, which extends a four block by one ply, and Rapfi, which extends it by 1.3.
- Everything else about a node is ordinary: the transposition table, the alpha-beta window and the move-count limit apply, so a forcing line is not searched twice.
- A move that makes **two threats at once** (a double four, or a four and an open three) is searched one ply deeper (`double_threat_ext`, switch `no-double`): that shape usually decides the game, and it is the one place where the search reaches further for a reason other than a forced reply.
- The answer to an open three may also be a counter-threat of ours — an open three of our own (`counter_threes`, switch `no-counters`). It is **off by default**: on the suite the wider answer set scores 19/21 against 20/21, so the narrower set stays.

On the suite (1 s per position) this scores 20/21 with a reported depth of 4 to 7. The reported depth is lower than before because each iteration now covers more plies: with `no-fours` it is 19/21 at 7 to 10, and with `no-double` it is the same 20/21. The suite has no attacking positions, so the double-threat extension was judged by matches, and there it came out level: on the 10–19 opening block it took the score against Tito from 4W–16L to 7W–13L, with pela unchanged at 4W–6L, but on the fresh 25–34 block it scored 3W–16L–1T against the pre-work control's 5W–15L — 10W–29L–1T against 9W–31L over 40 games per arm. It stays because it is principled and not measurably harmful, not because it is a gain.

## Evaluation (implemented)

`src/eval.rs`: for each side, its shape classes counted per direction, weighted 3000 (five), 1500 (open four), 300 (four), 100 (open three), 25 (blocked three) and 8 (two); the score is the side to move's total minus the opponent's, halved and clamped to +-10000. Counting per direction means a threat in two directions counts twice, and the blocked three and two classes give the score a development signal: a centre singleton used to score 0 and now scores 128, a blocked pair 121 against 425 for an open pair (`examples/audit.rs`). The halving keeps busy positions clear of the clamp (peak 4784 over 200 random 30-stone positions). The weights are untuned and there is no positional term yet.

## Implemented

- Bitboard board with make/unmake and win detection (`src/bitboard.rs`, `src/avx.rs`).
- Zobrist hashing (`src/zobrist.rs`): one 64-bit key per (player, cell), kept up to date by `Board::make` and `unmake`.
- Threat masks over empty cells (`src/threats.rs`):
  - `five_cells`: placing a stone makes five or more.
  - `four_cells`: placing a stone leaves a five-cell window with four own stones and one gap.
  - `open_four_cells`: placing a stone makes a run of exactly four with both ends empty (the board edge counts as blocked).
  - `open_three_cells`: placing a stone makes an open four possible next move using that stone.
  - `open_four_block_cells`: ends and gap of every open-four window; a superset of the real defences.
  - `three_and_two_cells`: placing a stone leaves a five-cell window with three own stones and two empty cells (threes), or two own stones and three empty cells (twos).
  - `Shapes`: for both players and each direction, the strongest class a stone would make out of five/open four/four/open three/blocked three/two, kept per direction so that a threat in two directions counts twice; this is what move ordering and the evaluation read.
- Piskvork protocol (`src/protocol.rs`): each move command is answered by running the search and replying with `DEBUG depth/score/nodes/time` followed by the move.
