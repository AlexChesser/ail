// @ts-check
'use strict';

const esbuild = require('esbuild');

const production = process.argv.includes('--production');
const watch = process.argv.includes('--watch');

/** @type {import('esbuild').BuildOptions} */
const options = {
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'dist/extension.js',
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  target: 'node12',
  sourcemap: !production,
  minify: production,
};

if (watch) {
  esbuild.context(options).then((ctx) => {
    ctx.watch();
    console.log('esbuild watching...');
  });
} else {
  esbuild.build(options).then(() => {
    console.log('esbuild: dist/extension.js');
  }).catch(() => process.exit(1));
}
