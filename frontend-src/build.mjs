import * as esbuild from 'esbuild';
import { readFileSync, writeFileSync, mkdirSync, cpSync } from 'fs';
import { resolve, dirname } from 'path';
import { fileURLToPath } from 'url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const outDir = resolve(__dirname, '../frontend');

mkdirSync(outDir, { recursive: true });

await esbuild.build({
  entryPoints: [resolve(__dirname, 'terminal.mjs')],
  bundle: true,
  format: 'esm',
  outfile: resolve(outDir, 'terminal.js'),
  minify: true,
  target: 'es2022',
});

// Copy CSS from @wterm/dom
const cssPath = resolve(__dirname, 'node_modules/@wterm/dom/src/terminal.css');
cpSync(cssPath, resolve(outDir, 'terminal.css'));

// Copy index.html
cpSync(resolve(__dirname, 'index.html'), resolve(outDir, 'index.html'));

console.log('Frontend built to ../frontend/');
