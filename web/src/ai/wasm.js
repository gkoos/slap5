// The engine, as the worker uses it: the page's occupancy words in, moves and per-iteration reports out.
//
// This replaces the JavaScript engine the page used to load (`ai/engine.js` and the twelve modules under
// it). The message contract did not change, so `ai/client.js` and the controller did not either - what
// changed is that the search is now the one the Piskvork binary runs, compiled to wasm.
import { BOARD_SIZE } from '../core/constants.js';
import init, { WasmEngine } from '../wasm/slap5_wasm.js';
import wasmUrl from '../wasm/slap5_wasm_bg.wasm?url';
import { level } from './difficulty.js';

// 2^19 entries of 16 bytes = 8 MB, the same table size the adapter defaults to. Each worker allocates its
// own, and every cancelled search starts a new worker, so this is deliberately smaller than the engine's
// native 32 MB.
const TT_BITS = 19;
const WORDS = 8;

let module = null;

/** Loads and instantiates the wasm module once per worker. */
function load() {
  // The URL comes from the bundler's `?url` import rather than from the glue's own default path, which is
  // what `--omit-default-module-path` in scripts/build-wasm.mjs removes.
  module ??= init({ module_or_path: wasmUrl });
  return module;
}

/** The adapter reads eight unsigned 32-bit words per colour; the page's bitboards hold signed-ish ones. */
function words(bitboard) {
  return Uint32Array.from(bitboard, (word) => word >>> 0);
}

export function createWasmEngine() {
  let engine = null;
  let loading = null;

  async function ensure() {
    if (engine) return engine;
    loading ??= load().then(() => {
      engine = new WasmEngine(TT_BITS);
      return engine;
    });
    return loading;
  }

  return {
    /**
     * Runs the search for a position and resolves with `{ row, col }`, or null when there is nothing to
     * play. `onProgress` gets a percentage per completed iteration, `onIteration` the iteration itself -
     * the client keeps the last one so it can play it when the user asks for the move early.
     */
    async findBestMove(position, difficulty, onProgress = () => {}, { onIteration } = {}) {
      const engine = await ensure();
      const { timeMs, maxDepth } = level(difficulty);
      engine.set_position(
        words(position.blackBitboard),
        words(position.whiteBitboard),
        position.toMove === 'black',
      );
      const move = engine.search(timeMs, maxDepth, (depth, x, y) => {
        onIteration?.({ depth, move: { row: y, col: x } });
        // Depth is the only progress a time-bounded search can report honestly: an iteration that is
        // still running when the clock stops is discarded, so the bar is "how deep is the move I have".
        onProgress(Math.min(95, Math.round((100 * depth) / maxDepth)));
      });
      onProgress(100);
      if (!move) return null;
      const { row, col } = move;
      // The result is a wasm allocation owned by JS: read it, then hand it back.
      move.free();
      return row >= 0 && row < BOARD_SIZE && col >= 0 && col < BOARD_SIZE ? { row, col } : null;
    },
  };
}
