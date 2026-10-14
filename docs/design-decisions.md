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
- **Only forced replies are free plies.** Attacking moves cost depth. Both PentaZen and Rapfi treat answering a four this way; neither makes the attacker's moves free.
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
