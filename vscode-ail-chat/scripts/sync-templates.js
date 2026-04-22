// @ts-check
'use strict';

/**
 * Copies demo template directories into vscode-ail-chat/templates/ so they
 * can be bundled into dist/ by esbuild. Run before every build.
 *
 * Source (tracked):  <repo-root>/demo/{starter,oh-my-ail,superpowers}/
 * Destination (gitignored):  vscode-ail-chat/templates/{starter,oh-my-ail,superpowers}/
 */

const fs = require('fs');
const path = require('path');

const repoRoot = path.resolve(__dirname, '..', '..');
const templatesDir = path.resolve(__dirname, '..', 'templates');
const demoDir = path.resolve(repoRoot, 'demo');

const TEMPLATES = ['starter', 'oh-my-ail', 'superpowers'];

function copyDir(src, dest) {
  fs.mkdirSync(dest, { recursive: true });
  for (const entry of fs.readdirSync(src, { withFileTypes: true })) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);
    if (entry.isDirectory()) {
      copyDir(srcPath, destPath);
    } else {
      fs.copyFileSync(srcPath, destPath);
    }
  }
}

fs.mkdirSync(templatesDir, { recursive: true });

for (const name of TEMPLATES) {
  const src = path.join(demoDir, name);
  const dest = path.join(templatesDir, name);
  if (!fs.existsSync(src)) {
    console.warn(`sync-templates: demo/${name} not found — skipping`);
    continue;
  }
  copyDir(src, dest);
  console.log(`sync-templates: demo/${name} → templates/${name}`);
}
