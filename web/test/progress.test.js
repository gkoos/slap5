// The progress bar's timing. The engine reports once per completed iteration, and iterations get further
// apart as it deepens, so the client interpolates between reports with the level's wall clock. These tests
// drive it with a fake clock and fake timers, which is what the injected `now`/`schedule` are for.
import assert from 'node:assert/strict';
import test from 'node:test';

import { createAiClient } from '../src/ai/client.js';
import { LEVELS } from '../src/ai/difficulty.js';

const TICK = 120;
const RESULT_DELAY = 150;

const emptyBoard = () => ({
  blackBitboard: Array(8).fill(0),
  whiteBitboard: Array(8).fill(0),
  toMove: 'black',
});

function harness() {
  const progress = [];
  const moves = [];
  const timers = new Map();
  let nextTimerId = 0;
  let clock = 0;
  const worker = {
    sent: [],
    terminated: false,
    postMessage(message) {
      this.sent.push(message);
    },
    terminate() {
      this.terminated = true;
    },
    reply(data) {
      this.onmessage({ data });
    },
  };
  const client = createAiClient({
    createWorker: () => worker,
    onProgress: (value) => progress.push(value),
    onMove: (move) => moves.push(move),
    schedule: (callback, delay) => {
      timers.set(++nextTimerId, { callback, delay });
      return nextTimerId;
    },
    cancelSchedule: (id) => timers.delete(id),
    now: () => clock,
  });
  return {
    client,
    worker,
    progress,
    moves,
    timers,
    setClock: (value) => {
      clock = value;
      return value;
    },
    // Fire the pending tick once, the way the browser would after 120 ms.
    tick: () => {
      for (const [id, timer] of [...timers])
        if (timer.delay === TICK && timers.has(id)) {
          timers.delete(id);
          timer.callback();
        }
    },
    request: (difficulty) => {
      client.findMove({ position: emptyBoard(), difficulty }, null);
      return client.pendingRequestId;
    },
    delays: () => [...timers.values()].map((timer) => timer.delay),
  };
}

test('the bar creeps with the clock between iteration reports', () => {
  const h = harness();
  h.client.initialize();
  const requestId = h.request('expert');
  const budget = LEVELS.expert.timeMs;

  // The first completed iteration arrives at once and sets the floor.
  h.worker.reply({ type: 'PROGRESS_UPDATE', requestId, progress: 20 });
  assert.equal(h.progress.at(-1), 20);

  // Then nothing for a long time - which is exactly when the bar used to stand still.
  h.setClock(budget / 2);
  h.tick();
  assert.equal(h.progress.at(-1), 50);

  h.setClock(budget * 0.75);
  h.tick();
  assert.equal(h.progress.at(-1), 75);
});

test('reports are a floor: the bar never moves backwards', () => {
  const h = harness();
  h.client.initialize();
  const requestId = h.request('expert');
  const budget = LEVELS.expert.timeMs;

  h.worker.reply({ type: 'PROGRESS_UPDATE', requestId, progress: 90 });
  assert.equal(h.progress.at(-1), 90);

  // The clock's estimate is behind the engine's own report, so the report stands.
  h.setClock(budget / 2);
  h.tick();
  assert.equal(h.progress.at(-1), 90);

  // And it stays believable: the bar cannot be full before the search has finished.
  h.setClock(budget * 4);
  h.tick();
  assert.equal(h.progress.at(-1), 95);

  assert.ok(
    h.progress.every((value, index) => index === 0 || value >= h.progress[index - 1]),
    `monotone: ${h.progress.join(',')}`,
  );
});

test('the pace comes from the level in difficulty.js, not from a fixed guess', () => {
  const h = harness();
  h.client.initialize();
  h.request('easy');
  h.setClock(LEVELS.easy.timeMs / 2);
  h.tick();
  assert.equal(h.progress.at(-1), 50, 'half of easy\'s budget is half the bar');
});

test('the bar advances inside the interval the next depth owns, without overtaking it', () => {
  const h = harness();
  h.client.initialize();
  const requestId = h.request('expert'); // 12 plies, so one depth is 8% of the bar

  // Depth 1 completes at 10 ms, depth 2 at 30 ms: the next iteration will take far longer than those.
  h.setClock(10);
  h.worker.reply({ type: 'SEARCH_ITERATION', requestId, depth: 1, move: { row: 7, col: 7 } });
  h.worker.reply({ type: 'PROGRESS_UPDATE', requestId, progress: 8 });
  h.setClock(30);
  h.worker.reply({ type: 'SEARCH_ITERATION', requestId, depth: 2, move: { row: 7, col: 8 } });
  h.worker.reply({ type: 'PROGRESS_UPDATE', requestId, progress: 17 });
  assert.equal(h.progress.at(-1), 17);

  h.setClock(90);
  h.tick();
  const early = h.progress.at(-1);
  h.setClock(240);
  h.tick();
  const later = h.progress.at(-1);

  assert.ok(early > 17, `past the last completed depth: ${early}`);
  assert.ok(later > early, `moving between reports: ${early} -> ${later}`);
  assert.ok(later < 25, `never claims the next depth (17% is depth 2, 25% is depth 3): ${later}`);
});

test('a finished search reports 100 and stops the creep', () => {
  const h = harness();
  h.client.initialize();
  const requestId = h.request('medium');
  h.worker.reply({ type: 'BEST_MOVE_FOUND', requestId, move: { row: 7, col: 7 } });
  assert.equal(h.progress.at(-1), 100);
  assert.deepEqual(h.delays(), [RESULT_DELAY], 'only the delayed move is left');

  const reported = h.progress.length;
  h.setClock(1_000_000);
  h.tick();
  assert.equal(h.progress.length, reported, 'the tick cannot fire once the search is done');
});

test('cancelling stops the creep', () => {
  const h = harness();
  h.client.initialize();
  h.request('expert');
  h.client.cancel();
  assert.equal(h.timers.size, 0, 'no tick left behind');

  const reported = h.progress.length;
  h.setClock(1_000_000);
  h.tick();
  assert.equal(h.progress.length, reported);
});
