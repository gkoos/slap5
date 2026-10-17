import { readFileSync } from 'node:fs';
import { defineConfig } from 'vite';

// The page states the version it was built from, read from the one place that version is written: the
// workspace manifest. A release bumps `[workspace.package] version` there and nothing here changes.
const manifest = readFileSync(new URL('../Cargo.toml', import.meta.url), 'utf8');
const version = /^version\s*=\s*"([^"]+)"/m.exec(manifest)?.[1] ?? 'unknown';

// The demo is deployed as a GitHub Pages project page, which lives under /<repo>/ rather than at the
// domain root, so the built asset URLs have to be prefixed. './' resolves under any prefix - a project
// page, a user page, a custom domain - and the workflow sets VITE_BASE to the repository's name as well,
// so the deployed URLs do not depend on relative-base behaviour.
export default defineConfig({
  base: process.env.VITE_BASE ?? './',
  define: { __SLAP5_VERSION__: JSON.stringify(version) },
});
