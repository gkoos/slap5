# Algorithm

Freestyle gomoku (15×15, five or more in a row wins), played over the Piskvork protocol. Status is marked per section.

## Search (planned)

Iterative-deepening alpha-beta (negamax). Each iteration searches one ply deeper than the last; when time runs out the best move of the last completed iteration is played, so a move is always available.

- Transposition table keyed by Zobrist hash (depth, bound, best move).
- Move ordering: TT move, then killer moves, then history heuristic; fours and threes before quiet moves.
- Candidates: empty cells within 2 of an existing stone.
- Time: a deadline checked every N nodes; the budget comes from `INFO timeout_turn`, `timeout_match` and `time_left`.
- Scores: wins and losses are scored by distance from the root so the shortest win is preferred.

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

## Evaluation (planned)

A pattern-count evaluation as a placeholder, tuned later.

## Implemented

- Bitboard board with make/unmake and win detection (`src/bitboard.rs`, `src/avx.rs`).
- Threat masks over empty cells (`src/threats.rs`):
  - `five_cells`: placing a stone makes five or more.
  - `four_cells`: placing a stone leaves a five-cell window with four own stones and one gap.
  - `open_four_cells`: placing a stone makes a run of exactly four with both ends empty (the board edge counts as blocked).
  - `open_three_cells`: placing a stone makes an open four possible next move using that stone.
  - `open_four_block_cells`: ends and gap of every open-four window; a superset of the real defences.
- Piskvork protocol (`src/protocol.rs`) with a placeholder mover (`src/mover.rs`) that plays the empty cell nearest the opponent's last stone.
