# Algorithm

Freestyle gomoku (15×15, five or more in a row wins), played over the Piskvork protocol. Status is marked per section.

## Search (baseline implemented)

`src/search.rs`: iterative-deepening principal variation search (negamax). Each iteration searches one ply deeper than the last. An iteration interrupted by the deadline is discarded entirely; the move comes from the last completed iteration, never from a partial one. Depth 1 is never interrupted, so a move always exists.

- Transposition table: 2^21 entries of 16 bytes (32 MB), keyed by the Zobrist hash plus the side to move; stores depth, bound, score and best move. Depth-preferred replacement within one search, anything is replaced once an entry is from an earlier search.
- Move ordering: table move first, then by a score from the threat masks of both sides (block a five 20000, own open four 10000, block an open four 5000, own four 2000, own open three 1000, block a four 800, block an open three 500, summed), plus 250/200 for making/blocking a three and 40/30 for a two in a window of five, plus 4000/3000 for the two killer moves of the ply and the history heuristic. The three/two part is quartered when the cell is more than 4 cells from each of the last four moves of the current line, and halved when it is more than 4 from each of the last two.
- Quiet moves: a move that makes or blocks no five, four or open three and is not a killer. At nodes that are not the root and not forced, moves past a move-count limit are skipped (at ply 1 only quiet ones): `round(15*sqrt(d) - 12)` clamped to 3..32, or `round(5*sqrt(d) - 2)` clamped to 2..8 when a four or open three is on the board. Nothing is skipped while every move searched so far loses.
- Late move reductions: from the fourth move, quiet moves at depth 3 or more are searched 1 ply shallower (2 from the ninth move at depth 6 or more, plus 1 at null-window nodes), and again at full depth if they beat alpha. `selective` switches pruning and reductions off together, which makes the search exact.
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

A reply to a four is not free: the leaf search follows fours, so a free block would only repeat it. A move that creates a threat costs a ply like any other. A double three or four-three is not treated as a win by rule; the search proves or refutes it. Forced nodes are expanded even when the remaining depth is 0. Free plies stop at ply 64 from the root (`free_ply_limit`), after which a forced node costs depth like any other, and the deadline check applies inside chains. The first iteration (depth 1) is plain, so it stays cheap and always completes. `Stats::forced_nodes` counts the forced nodes.

`free_three_replies` defaults to on. The blocking cells for an open three are a superset of the real defences (see `open_four_block_cells`), which only adds candidates.

## Leaf search for fours (implemented)

At remaining depth 0 (and not a free ply) the side to move does not evaluate at once. `Searcher::leaf` is a small alpha-beta over fours only:
- If the opponent threatens five, the block is the only move and it is free (two five cells means the side loses).
- Otherwise the side may stand pat (take the static evaluation) or play any four (an open four never gets here: the decided-position rules score it as a win). Each four forces a single reply, so the chain stays narrow.
- At most `leaf_depth` voluntary fours (default 1); the deadline check applies inside it. It is off in the first iteration, like free plies, and `leaf_depth = 0` switches it off.
- Results are not stored in the transposition table. `Stats::leaf_nodes` counts its calls.

## Evaluation (placeholder implemented)

`src/eval.rs`: for each side, a weighted count of the cells where it could make a five (3000), an open four (1500), a four (300) or an open three (100); the score is the side to move's total minus the opponent's, clamped to +-10000. The weights are untuned and there is no positional term yet.

## Implemented

- Bitboard board with make/unmake and win detection (`src/bitboard.rs`, `src/avx.rs`).
- Zobrist hashing (`src/zobrist.rs`): one 64-bit key per (player, cell), kept up to date by `Board::make` and `unmake`.
- Threat masks over empty cells (`src/threats.rs`):
  - `five_cells`: placing a stone makes five or more.
  - `four_cells`: placing a stone leaves a five-cell window with four own stones and one gap.
  - `open_four_cells`: placing a stone makes a run of exactly four with both ends empty (the board edge counts as blocked).
  - `open_three_cells`: placing a stone makes an open four possible next move using that stone.
  - `open_four_block_cells`: ends and gap of every open-four window; a superset of the real defences.
  - `three_and_two_cells`: placing a stone leaves a five-cell window with three own stones and two empty cells (threes), or two own stones and three empty cells (twos); used by move ordering.
- Piskvork protocol (`src/protocol.rs`): each move command is answered by running the search and replying with `DEBUG depth/score/nodes/time` followed by the move.
