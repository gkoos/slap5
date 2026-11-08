// The page's side of the adapter's wire format: occupancy words in, a move back.
//
// It sits apart from `wasm.js` for the reason the Rust side splits `decode`/`run` from its exported
// methods: `wasm.js` reaches the adapter through the bundler's `?url` import, which plain Node cannot
// resolve, so nothing that lives in it can be covered by `npm test`. Both of these are the parts that can
// be wrong in a way nothing else notices.
//
// `toMove` in particular: the adapter exposes `row()` and `col()` as *methods*, and reading them as
// properties hands back two functions, which turns every finished search into "no move" - the worker then
// reports a null move and `client.js` fills in the controller's random move. v0.2.0 shipped that, and
// `web/test/wasm-engine.test.js` drives the real adapter through this function so it cannot happen again.
import { BOARD_SIZE } from '../core/constants.js';

/** The adapter reads eight unsigned 32-bit words per colour; the page's bitboards hold signed-ish ones. */
export function words(bitboard) {
  return Uint32Array.from(bitboard, (word) => word >>> 0);
}

/** The adapter's finished search as the page's move, or null if it cannot be one. */
export function toMove(result, size = BOARD_SIZE) {
  const row = result.row();
  const col = result.col();
  return Number.isInteger(row) &&
    Number.isInteger(col) &&
    row >= 0 &&
    row < size &&
    col >= 0 &&
    col < size
    ? { row, col }
    : null;
}
