// The worker's message contract. The engine is injected, so this runs without wasm: the adapter between
// the two - the bitboard words, the callback bridging and the coordinate swap - is covered natively by
// `cargo test -p slap5-wasm` and in the browser by playing a game.
import assert from 'node:assert/strict';
import test from 'node:test';

import { createWorkerHandler } from '../src/ai/worker-handler.js';
import { BOARD_SIZE } from '../src/core/constants.js';
import { board2Bitboards } from '../src/core/bitboards.js';
import { firstCandidate } from '../src/core/moves.js';

function grid(fill = null) {
  return Array.from({ length: BOARD_SIZE }, () => Array(BOARD_SIZE).fill(fill));
}

function position() {
  return { ...board2Bitboards(grid()), toMove: 'black' };
}

function fakeEngine({ move = { row: 7, col: 7 }, iterations = [], fail = null } = {}) {
  const calls = [];
  return {
    calls,
    async findBestMove(asked, difficulty, onProgress, { onIteration } = {}) {
      calls.push({ position: asked, difficulty });
      if (fail) throw fail;
      for (const iteration of iterations) {
        onIteration(iteration);
        onProgress(50);
      }
      return move;
    },
  };
}

function harness(engine) {
  const messages = [];
  const errors = [];
  const handler = createWorkerHandler({
    postMessage: (message) => messages.push(message),
    createEngine: () => engine,
    reportError: (...args) => errors.push(args),
  });
  return {
    messages,
    errors,
    request: (requestId, data, type = 'FIND_BEST_MOVE') =>
      handler({ data: { type, requestId, data } }),
  };
}

test('replies echo the request id and every completed iteration is reported', async () => {
  const engine = fakeEngine({
    iterations: [
      { depth: 1, move: { row: 7, col: 7 } },
      { depth: 2, move: { row: 7, col: 8 } },
    ],
  });
  const { request, messages } = harness(engine);
  await request(3, { position: position(), difficulty: 'hard' });

  assert.deepEqual(
    messages.filter((message) => message.type === 'SEARCH_ITERATION'),
    [
      { type: 'SEARCH_ITERATION', requestId: 3, depth: 1, move: { row: 7, col: 7 } },
      { type: 'SEARCH_ITERATION', requestId: 3, depth: 2, move: { row: 7, col: 8 } },
    ],
  );
  assert.ok(messages.some((message) => message.type === 'PROGRESS_UPDATE'));
  assert.deepEqual(messages.at(-1), {
    type: 'BEST_MOVE_FOUND',
    requestId: 3,
    move: { row: 7, col: 7 },
  });
  assert.ok(
    messages.every((message) => message.requestId === 3),
    'the client drops replies that are not the request it waits for',
  );
  assert.equal(engine.calls.length, 1);
  assert.equal(engine.calls[0].difficulty, 'hard');
  assert.equal(engine.calls[0].position.toMove, 'black');
});

test('no answer from the engine is reported as a null move', async () => {
  const { request, messages } = harness(fakeEngine({ move: null }));
  await request(1, { position: position(), difficulty: 'easy' });
  assert.deepEqual(messages.at(-1), { type: 'BEST_MOVE_FOUND', requestId: 1, move: null });
});

test('a request without a position is answered without a search', async () => {
  const engine = fakeEngine();
  const { request, messages } = harness(engine);
  await request(2, { position: { toMove: 'black' }, difficulty: 'easy' });
  assert.deepEqual(messages, [{ type: 'BEST_MOVE_FOUND', requestId: 2, move: null }]);
  assert.equal(engine.calls.length, 0);
});

test('an engine that rejects the position falls back to the first legal cell', async () => {
  const engine = fakeEngine({ fail: new Error('both colours on cell') });
  const { request, messages, errors } = harness(engine);
  await request(4, { position: position(), difficulty: 'easy' });
  assert.equal(errors.length, 1, 'the rejection is reported, not swallowed');
  assert.deepEqual(messages, [
    { type: 'BEST_MOVE_FOUND', requestId: 4, move: { row: 0, col: 0 } },
  ]);
});

test('messages other than FIND_BEST_MOVE are ignored', async () => {
  const engine = fakeEngine();
  const { request, messages } = harness(engine);
  await request(5, { position: position() }, 'NEW_GAME');
  assert.deepEqual(messages, []);
  assert.equal(engine.calls.length, 0);
});

test('one engine serves every move of the game', async () => {
  const engine = fakeEngine();
  const messages = [];
  let created = 0;
  const handler = createWorkerHandler({
    postMessage: (message) => messages.push(message),
    createEngine: async () => {
      created += 1;
      return engine;
    },
    reportError() {},
  });
  for (const requestId of [1, 2, 3])
    await handler({
      data: {
        type: 'FIND_BEST_MOVE',
        requestId,
        data: { position: position(), difficulty: 'medium' },
      },
    });
  assert.equal(created, 1, 'the transposition table survives between moves');
  assert.equal(engine.calls.length, 3);
});

test('the first candidate is legal, central and next to a stone', () => {
  assert.deepEqual(firstCandidate(grid()), { row: 7, col: 7 }, 'the centre of an empty board');

  const corner = grid();
  corner[0][0] = 'black';
  assert.deepEqual(firstCandidate(corner), { row: 0, col: 1 });

  const full = grid().map((row, r) => row.map((_, c) => ((r + c) % 2 ? 'black' : 'white')));
  assert.equal(firstCandidate(full), null, 'nothing to play on a full board');

  const hole = full.map((row) => [...row]);
  hole[5][5] = null;
  assert.deepEqual(firstCandidate(hole), { row: 5, col: 5 }, 'the only cell left');
});
