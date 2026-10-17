import { BOARD_SIZE } from './constants.js';

const CENTRE = Math.floor(BOARD_SIZE / 2);

/**
 * The move to play before the engine has an answer, or if it fails. The search replaces this as soon as
 * its first iteration finishes, so it only has to be legal and not absurd: the centre on an empty board,
 * otherwise the first empty cell next to a stone in reading order, otherwise the first empty cell.
 *
 * It replaces the candidate ordering the JavaScript engine used to expose - that ordering existed to feed
 * its own search, and the wasm engine generates its own candidates.
 */
export function firstCandidate(board) {
  let anywhere = null;
  let empty = true;
  for (let row = 0; row < BOARD_SIZE; row++) {
    for (let col = 0; col < BOARD_SIZE; col++) {
      if (board[row][col] !== null) {
        empty = false;
        continue;
      }
      anywhere ??= { row, col };
      for (let dr = -1; dr <= 1; dr++) {
        for (let dc = -1; dc <= 1; dc++) {
          if (!dr && !dc) continue;
          const r = row + dr;
          const c = col + dc;
          if (
            r >= 0 &&
            r < BOARD_SIZE &&
            c >= 0 &&
            c < BOARD_SIZE &&
            board[r][c] !== null
          )
            return { row, col };
        }
      }
    }
  }
  return empty ? { row: CENTRE, col: CENTRE } : anywhere;
}
