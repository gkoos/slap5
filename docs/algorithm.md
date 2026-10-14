# Algorithm

Freestyle gomoku (15×15, five or more in a row wins), played over the Piskvork protocol. Status is marked per section.

## Search (baseline implemented)

`src/search.rs`: iterative-deepening alpha-beta (negamax, no PVS yet). Each iteration searches one ply deeper than the last. An iteration interrupted by the deadline is discarded entirely; the move comes from the last completed iteration, never from a partial one. Depth 1 is never interrupted, so a move always exists.

- Transposition table: 2^21 entries of 16 bytes (32 MB), keyed by the Zobrist hash plus the side to move; stores depth, bound, score and best move. Depth-preferred replacement within one search, anything is replaced once an entry is from an earlier search.
- Move ordering: table move, then the cells that block the opponent's five, our open-four cells, the opponent's open-four cells, two killer moves per ply, then the history heuristic.
- Candidates: empty cells within 2 of an existing stone. On an empty board the engine plays the centre.
- Terminal rules at a node: a full board is a draw; a side that can make five scores a win (checked before the depth test); at depth 0 the static evaluation is returned.
- Scores: a win at ply p scores `30000 - p`, so the shortest win is preferred. Table entries store mate scores relative to the node.
- Time: the deadline is checked every 1024 nodes, and no new iteration starts once half the budget is used. The budget is `INFO timeout_turn` minus 50 ms, at most a tenth of `time_left` (a placeholder for proper time management).

## Forced moves do not cost depth (planned)

When the side to move is forced, the node is searched without reducing the depth counter. The first applicable level decides the candidates:

| Level | Side to move | Candidates | Depth |
|---|---|---|---|
| 1 | can make five | the five | terminal |
| 2 | opponent has a four | the blocking cell | free |
| 3 | can make an open four or double four | that move | decisive win |
| 4 | opponent has an open three | blocking cells plus own fours | free |
| 5 | none of the above | ordinary moves; fours and threes ordered first | costs one ply |

Only forced replies are free. A move that creates a threat costs a ply like any other. A double three or four-three is not treated as a win by rule; the search proves or refutes it. Forced nodes are expanded even when the remaining depth is 0. A cap on free plies from the root bounds chains, and the time check applies inside them.

## Leaf search for fours (planned)

At remaining depth 0 the side to move tries only fours, each forcing a single reply (VCF), instead of evaluating statically. It may stop and take the static evaluation (stand-pat). It has its own node and ply limit and checks time.

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
- Piskvork protocol (`src/protocol.rs`): each move command is answered by running the search and replying with `DEBUG depth/score/nodes/time` followed by the move.
