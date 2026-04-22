// @ts-check
'use strict';

const esbuild = require('esbuild');
const fs = require('fs');
const path = require('path');

const production = process.argv.includes('--production');
const watch = process.argv.includes('--watch');

/** @type {import('esbuild').BuildOptions} */
const extensionOptions = {
  entryPoints: ['src/extension.ts'],
  bundle: true,
  outfile: 'dist/extension.js',
  external: ['vscode'],
  format: 'cjs',
  platform: 'node',
  target: 'node16',
  sourcemap: !production,
  minify: production,
};

/** @type {import('esbuild').BuildOptions} */
const webviewOptions = {
  entryPoints: ['src/webview/index.tsx'],
  bundle: true,
  outfile: 'dist/webview.js',
  format: 'iife',
  platform: 'browser',
  target: 'es2020',
  sourcemap: !production,
  minify: production,
};

/** @type {import('esbuild').BuildOptions} */
const graphWebviewOptions = {
  entryPoints: ['src/webview-graph/index.tsx'],
  bundle: true,
  outfile: 'dist/graphWebview.js',
  format: 'iife',
  platform: 'browser',
  target: 'es2020',
  sourcemap: !production,
  minify: production,
  // React Flow ships CSS that we inline via the css loader.
  loader: { '.css': 'css' },
};

function copyTemplates() {
  const src = path.join(__dirname, 'templates');
  const dest = path.join(__dirname, 'dist', 'templates');
  if (!fs.existsSync(src)) return;

  function copyDir(s, d) {
    fs.mkdirSync(d, { recursive: true });
    for (const entry of fs.readdirSync(s, { withFileTypes: true })) {
      const sp = path.join(s, entry.name);
      const dp = path.join(d, entry.name);
      if (entry.isDirectory()) copyDir(sp, dp);
      else fs.copyFileSync(sp, dp);
    }
  }
  copyDir(src, dest);
}

function copyCodiconAssets() {
  const dist = path.join(__dirname, 'dist');
  if (!fs.existsSync(dist)) fs.mkdirSync(dist, { recursive: true });

  // Copy codicon CSS (rewrite font url to point to local file)
  const codiconCssSrc = path.join(__dirname, 'node_modules', '@vscode', 'codicons', 'dist', 'codicon.css');
  const codiconFontSrc = path.join(__dirname, 'node_modules', '@vscode', 'codicons', 'dist', 'codicon.ttf');

  if (fs.existsSync(codiconCssSrc)) {
    let css = fs.readFileSync(codiconCssSrc, 'utf8');
    // Ensure the font URL points to the same dist directory
    css = css.replace(/url\(['"]?([^'")\s]+)['"]?\)/g, "url('./codicon.ttf')");
    fs.writeFileSync(path.join(dist, 'codicon.css'), css);
  }
  if (fs.existsSync(codiconFontSrc)) {
    fs.copyFileSync(codiconFontSrc, path.join(dist, 'codicon.ttf'));
  }
}

async function build() {
  if (!watch) {
    // Clean dist before every non-watch build (rmdirSync works on Node ≥12.10)
    const dist = path.join(__dirname, 'dist');
    try {
      (fs.rmSync || fs.rmdirSync)(dist, { recursive: true, force: true });
    } catch (_) {}
  }

  copyCodiconAssets();
  copyTemplates();

  if (watch) {
    const [extCtx, webCtx, graphCtx] = await Promise.all([
      esbuild.context(extensionOptions),
      esbuild.context(webviewOptions),
      esbuild.context(graphWebviewOptions),
    ]);
    await Promise.all([extCtx.watch(), webCtx.watch(), graphCtx.watch()]);
    console.log('esbuild watching...');
  } else {
    await Promise.all([
      esbuild.build(extensionOptions),
      esbuild.build(webviewOptions),
      esbuild.build(graphWebviewOptions),
    ]);
    console.log('esbuild: dist/extension.js + dist/webview.js + dist/graphWebview.js');
  }
}

build().catch(() => process.exit(1));
