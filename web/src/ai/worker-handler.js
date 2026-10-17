import { findLegalFallback } from '../core/rules.js';

/**
 * The worker's message handler: the contract the JavaScript engine used to answer, with the wasm search
 * behind it.
 *
 *   FIND_BEST_MOVE   { requestId, data: { position, difficulty } }   from the page
 *   PROGRESS_UPDATE  { requestId, progress }                         per completed iteration
 *   SEARCH_ITERATION { requestId, depth, move }                      per completed iteration
 *   BEST_MOVE_FOUND  { requestId, move }                             once, ends the request
 *
 * Every reply carries the request id back: the client drops anything that is not the request it is
 * waiting for, which is how a terminated worker's late answers are ignored.
 *
 * `createEngine` is passed in rather than imported, which is what keeps this file free of the wasm module
 * and its bundler-only asset import: `worker.js` supplies the real engine and the tests supply a fake.
 */
export function createWorkerHandler({
  postMessage,
  createEngine,
  reportError = console.error,
}) {
  let engine = null;
  return async function (event) {
    const requestId = event.data?.requestId;
    try {
      const { type, data } = event.data ?? {};
      if (type !== 'FIND_BEST_MOVE') return;
      const { blackBitboard, whiteBitboard, toMove } = data.position || data;
      const computerPlayer = toMove || data.computerPlayer;
      if (!blackBitboard || !whiteBitboard) {
        postMessage({ type: 'BEST_MOVE_FOUND', requestId, move: null });
        return;
      }
      // One engine per worker, created on the first request: it holds the transposition table and the
      // move-ordering history, which are meant to survive between the moves of a game.
      engine ??= await createEngine();
      const move = await engine.findBestMove(
        { blackBitboard, whiteBitboard, toMove: computerPlayer },
        data.difficulty,
        (progress) => postMessage({ type: 'PROGRESS_UPDATE', requestId, progress }),
        {
          onIteration: (iteration) =>
            postMessage({ type: 'SEARCH_ITERATION', requestId, ...iteration }),
        },
      );
      postMessage({ type: 'BEST_MOVE_FOUND', requestId, move });
    } catch (error) {
      // The page needs a move even when the engine cannot give one, and the client already holds a legal
      // fallback: rejecting the position (an overlap, a short word array) must not stall the game.
      reportError('AI Worker error:', error);
      const position = event.data?.data?.position ?? event.data?.data ?? {};
      postMessage({
        type: 'BEST_MOVE_FOUND',
        requestId,
        move: findLegalFallback(position.blackBitboard, position.whiteBitboard),
      });
    }
  };
}
