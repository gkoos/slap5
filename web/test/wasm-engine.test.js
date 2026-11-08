// The adapter as the page runs it: the real wasm module, loaded in Node, driven through the page's own
// wire helpers.
//
// This is the boundary the rest of the suite leaves out. A fake engine in `worker-adapter.test.js` cannot
// see that the adapter's accessors are methods rather than properties, and that is how v0.2.0 shipped a
// page where every finished search resolved to "no move": the worker reported a null move and the
// controller filled in a random one, which reads as an engine that ignores the position. The generated
// package is not committed, so the wasm cases skip until `npm run build:wasm` has run - the test job of
// `.github/workflows/pages.yml` builds it first precisely so they do not.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { LEVELS } from '../src/ai/difficulty.js';
import { toMove, words } from '../src/ai/wire.js';
import { board2Bitboards } from '../src/core/bitboards.js';
import { BOARD_SIZE } from '../src/core/constants.js';

const dir = fileURLToPath(new URL('../src/wasm/', import.meta.url));
const built = fs.existsSync(`${dir}slap5_wasm.js`) && fs.existsSync(`${dir}slap5_wasm_bg.wasm`);
const skip = built ? false : 'no generated wasm package: run `npm run build:wasm`';

// `initSync` over the file's bytes rather than the page's `init(url)`: Node does not fetch a `file:` URL,
// and the `?url` import that supplies it in the browser is a bundler form this test does not need.
const glue = built ? await import(new URL('../src/wasm/slap5_wasm.js', import.meta.url).href) : null;
if (glue) glue.initSync({ module: fs.readFileSync(`${dir}slap5_wasm_bg.wasm`) });

/** The page's board, as `board2Bitboards` reads it. */
function position(black, white, toMove) {
  const board = Array.from({ length: BOARD_SIZE }, () => Array(BOARD_SIZE).fill(null));
  for (const [r, c] of black) board[r][c] = 'black';
  for (const [r, c] of white) board[r][c] = 'white';
  return { ...board2Bitboards(board), toMove };
}

/** One move, asked for the way `wasm.js` asks: the page's position in, the level's budget. */
function move(pos, difficulty) {
  const { timeMs, maxDepth } = LEVELS[difficulty];
  const engine = new glue.WasmEngine(19);
  engine.set_position(words(pos.blackBitboard), words(pos.whiteBitboard), pos.toMove === 'black');
  const result = engine.search(timeMs, maxDepth, () => {});
  assert.ok(result, `${difficulty}: a search on a non-empty board returns a result`);
  const chosen = toMove(result);
  result.free();
  engine.free();
  assert.ok(chosen, `${difficulty}: the result reads as a move - the accessors are called, not read`);
  return chosen;
}

test('the occupancy words are the unsigned form the adapter takes', () => {
  // The page's bitboards carry bitwise-op results, so word 6 is `1 << 31` and negative as an int32.
  const signed = [0, 0, 0, 0, 0, 0, -2147483648, 0];
  assert.deepEqual([...words(signed)], [0, 0, 0, 0, 0, 0, 2147483648, 0]);
  assert.ok(words([-1]) instanceof Uint32Array, 'a typed array is what the wasm argument wants');
});

test('a five that is available now is played', { skip }, () => {
  const pos = position([[7, 3], [7, 4], [7, 5], [7, 6]], [[10, 10], [11, 10]], 'black');
  const chosen = move(pos, 'easy');
  assert.ok(
    ['7,2', '7,7'].includes(`${chosen.row},${chosen.col}`),
    `(${chosen.row},${chosen.col}) completes the five`,
  );
});

// Black has (7,5),(7,6),(7,7) with both ends of the row free: white takes (7,4) or (7,8) or black plays
// the other end next move and makes an open four.
test('a live three is answered', { skip }, () => {
  const pos = position([[7, 5], [7, 6], [7, 7]], [[6, 6], [8, 6]], 'white');
  for (const difficulty of ['easy', 'medium']) {
    const chosen = move(pos, difficulty);
    assert.ok(
      ['7,4', '7,8'].includes(`${chosen.row},${chosen.col}`),
      `${difficulty}: (${chosen.row},${chosen.col}) answers the three`,
    );
  }
});

// The same threat with the colours the other way round, so both sides of the position's `toMove` reach
// the adapter.
test('a live three is answered with black to move', { skip }, () => {
  const pos = position([[9, 6], [5, 6]], [[8, 5], [8, 6], [8, 7]], 'black');
  const chosen = move(pos, 'easy');
  assert.ok(
    ['8,4', '8,8'].includes(`${chosen.row},${chosen.col}`),
    `(${chosen.row},${chosen.col}) answers the three`,
  );
});
