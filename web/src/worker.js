import { createWorkerHandler } from './ai/worker-handler.js';
import { createWasmEngine } from './ai/wasm.js';

// The engine is chosen here, in the worker, so that `ai/worker-handler.js` stays free of the wasm module
// (whose `.wasm` import is a bundler-only URL that plain Node cannot resolve) and can be tested directly.
self.addEventListener(
  'message',
  createWorkerHandler({
    createEngine: () => createWasmEngine(),
    postMessage: (message) => self.postMessage(message),
  }),
);
