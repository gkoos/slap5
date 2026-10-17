// Measures what each difficulty level actually reaches with the real wasm build, on positions from the
// engine's suite file. Run from web/ so the relative import below resolves.
import { readFileSync } from 'node:fs';
import init, { WasmEngine } from '../src/wasm/slap5_wasm.js';
import { LEVELS } from '../src/ai/difficulty.js';

const SIZE = 15;
await init({ module_or_path: readFileSync(new URL('../src/wasm/slap5_wasm_bg.wasm', import.meta.url)) });

// The suite's line format: name us=<side> stones=<p>:<x>,<y> ... played=x,y saving=x,y ...
const lines = readFileSync(new URL('../../engine/examples/last_chance.txt', import.meta.url), 'utf8')
  .split('\n')
  .filter((line) => line.trim() && !line.startsWith('#'));

function parse(line) {
  const parts = line.split(/\s+/);
  const name = parts.shift();
  let us = 0;
  let field = '';
  const stones = [];
  for (const token of parts) {
    if (token.startsWith('us=')) { us = Number(token.slice(3)); field = 'us'; }
    else if (token.startsWith('stones=')) { stones.push(token.slice(7)); field = 'stones'; }
    else if (token.startsWith('played=')) field = 'played';
    else if (token.startsWith('saving=')) field = 'saving';
    else if (field === 'stones') stones.push(token);
  }
  return { name, us, stones };
}

function words(stones, side) {
  const word = new Uint32Array(8);
  for (const stone of stones) {
    const [p, xy] = stone.split(':');
    if (Number(p) !== side) continue;
    const [x, y] = xy.split(',').map(Number);
    const cell = y * SIZE + x;
    word[cell >>> 5] |= 1 << (cell & 31);
  }
  return word;
}

const positions = lines.slice(0, 3).map(parse);
for (const level of Object.keys(LEVELS)) {
  const { timeMs, maxDepth } = LEVELS[level];
  for (const position of positions) {
    const engine = new WasmEngine(19);
    engine.set_position(words(position.stones, 0), words(position.stones, 1), position.us === 0);
    let last = null;
    const started = performance.now();
    const move = engine.search(timeMs, maxDepth, (depth, x, y) => { last = { depth, x, y }; });
    const elapsed = Math.round(performance.now() - started);
    console.log(
      `${level.padEnd(6)} budget ${String(timeMs).padStart(4)}ms/${String(maxDepth).padStart(2)}ply  ` +
        `${position.name.slice(0, 26).padEnd(26)} -> depth ${String(move?.depth).padStart(2)} ` +
        `move ${last ? `${last.y},${last.x}` : '-'} nodes ${String(move?.nodes).padStart(9)} ` +
        `score ${String(move?.score).padStart(6)} elapsed ${elapsed}ms`,
    );
    move?.free();
    engine.free();
  }
}
