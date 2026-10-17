import './style.css';
import { createGameController } from './game/controller.js';

// The engine runs in a worker: a wasm search is synchronous and would freeze the page otherwise, and
// terminating the worker is the only way to stop one mid-thought, which is what "Move now" and "New Game"
// do.
const game = createGameController({
  document,
  createWorker: () =>
    new Worker(new URL('./worker.js', import.meta.url), { type: 'module' }),
});
game.initGame();
window.addEventListener('beforeunload', game.cleanup);
