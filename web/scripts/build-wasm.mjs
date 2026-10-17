#!/usr/bin/env node
// Builds the engine for the browser and post-processes it into web/src/wasm/.
//
// Two things have to agree for wasm-bindgen to work at all: the `wasm-bindgen` crate the adapter is
// linked against, and the `wasm-bindgen` CLI that rewrites the binary. They are pinned to the same
// version in the manifests, and this script stops if the CLI it finds is a different one - otherwise the
// failure surfaces much later as a schema-version error from inside the CLI.
//
// Plain Node rather than PowerShell, so the same command runs on a developer's machine and on the ubuntu
// runner that deploys the page.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url)); // web/scripts
const web = resolve(here, '..'); // web/
const root = resolve(web, '..'); // the workspace root
const crate = 'crates/slap5-wasm';
const target = 'wasm32-unknown-unknown';
// `wasm-release` is the workspace profile that adds LTO for the wasm build alone; the engine's own
// release profile, the one matches are measured with, is untouched.
const profile = 'wasm-release';
const outDir = join(web, 'src', 'wasm');
const outName = 'slap5_wasm';

function output(command, args) {
  return execFileSync(command, args, { encoding: 'utf8' }).trim();
}

// The resolved version from the lock file, not the requirement: that is what is linked in.
const lock = readFileSync(join(root, 'Cargo.lock'), 'utf8');
const linked = /name = "wasm-bindgen"\r?\nversion = "([^"]+)"/.exec(lock)?.[1];
const cli = /wasm-bindgen (\d+\.\d+\.\d+)/.exec(output('wasm-bindgen', ['--version']))?.[1];
if (!linked) throw new Error('wasm-bindgen is not in Cargo.lock');
if (linked !== cli) {
  throw new Error(
    `the adapter links wasm-bindgen ${linked} but the CLI is ${cli}: ` +
      `install it with "cargo install wasm-bindgen-cli --version ${linked} --locked"`,
  );
}

execFileSync('cargo', ['build', '-p', 'slap5-wasm', '--profile', profile, '--target', target], {
  cwd: root,
  stdio: 'inherit',
});

mkdirSync(outDir, { recursive: true });
execFileSync(
  'wasm-bindgen',
  [
    '--target',
    'web',
    '--out-dir',
    outDir,
    '--out-name',
    outName,
    // The page imports the .wasm through the bundler (`?url`) and hands the URL to `init`, so the glue's
    // own `new URL(..., import.meta.url)` fallback is not used: inside a worker chunk that depends on the
    // bundler rewriting it, and an explicit path fails loudly instead of fetching the wrong thing.
    '--omit-default-module-path',
    join(root, 'target', target, profile, `${outName}.wasm`),
  ],
  { cwd: root, stdio: 'inherit' },
);

console.log(`\nweb/src/wasm/${outName}.js and .wasm built from ${crate} with wasm-bindgen ${cli}`);
