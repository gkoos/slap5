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

// The version this page was built from, injected from the workspace manifest by vite.config.js. It ties a
// deployed page to a git tag, so "which engine is live" needs no guessing.
const version = document.getElementById('site-version');
if (version) version.textContent = `slap5 ${__SLAP5_VERSION__}`;
