import { defineConfig } from 'vite';

// The demo is deployed as a GitHub Pages project page, which lives under /<repo>/ rather than at the
// domain root, so the built asset URLs have to be prefixed. './' resolves under any prefix - a project
// page, a user page, a custom domain - and the workflow sets VITE_BASE to the repository's name as well,
// so the deployed URLs do not depend on relative-base behaviour.
export default defineConfig({ base: process.env.VITE_BASE ?? './' });
