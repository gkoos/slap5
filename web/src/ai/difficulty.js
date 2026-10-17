// The search budgets behind the difficulty selector.
//
// The engine is time-bounded and iterative: it deepens until the clock runs out or the depth cap is
// reached, whichever comes first, and reports every iteration that finished. A level is therefore a
// (time, depth) pair - the time is what the level is worth waiting for and the depth is the ceiling, which
// also gives the progress bar something to measure against.
//
// The JavaScript engine this replaces searched 6, 8 and 10 plies at medium, hard and expert, with a
// heuristic-only easy. The depths here follow it; the times are what the wasm build needs to reach them
// (the browser build is slower than the native one: it has no AVX2, so it uses the scalar board).
export const LEVELS = {
  easy: { timeMs: 300, maxDepth: 2 },
  medium: { timeMs: 1000, maxDepth: 6 },
  hard: { timeMs: 2500, maxDepth: 8 },
  expert: { timeMs: 8000, maxDepth: 12 },
};

export function level(difficulty) {
  return LEVELS[difficulty] ?? LEVELS.medium;
}
