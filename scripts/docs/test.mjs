import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
import {execFileSync} from 'node:child_process';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = path.join(root, 'docs');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'scripts/docs/pages.json')));
const pages = manifest.groups.flatMap(group => group.pages);
const files = [...pages.map(id => `tutorial/${id}.html`), ...Object.keys(manifest.aliases)];
const errors = [];
const decode = value => value.replace(/&amp;/g, '&').replace(/&#39;/g, "'").replace(/&quot;/g, '"');
let links = 0;
for (const filename of files) {
  const html = fs.readFileSync(path.join(docs, filename), 'utf8');
  const ids = [...html.matchAll(/\bid="([^"]+)"/g)].map(match => match[1]);
  const check = (condition, message) => { if (!condition) errors.push(`${filename}: ${message}`); };
  check(new Set(ids).size === ids.length, 'Duplicate IDs');
  check((html.match(/<h1\b/g) || []).length === 1, 'Must have one H1');
  check((html.match(/aria-current="page"/g) || []).length === 1, 'Must have one current navigation item');
  check(html.includes('name="description"'), 'Missing description');
  check(html.includes('rel="canonical"'), 'Missing canonical');
  check(html.includes('og-many-minds.png'), 'Missing shared social image');
  check(html.includes('Skip to content'), 'Missing skip link');
  check(html.includes('prefers-reduced-motion') || fs.readFileSync(path.join(docs, 'tutorial/style.css'), 'utf8').includes('prefers-reduced-motion'), 'Missing reduced-motion handling');
  for (const match of html.matchAll(/\b(?:href|src)="([^"]+)"/g)) {
    const value = decode(match[1]);
    if (/^(?:https?:|mailto:|data:)/.test(value)) continue;
    links++;
    const [linkPath, fragment] = value.split('#');
    let destination = linkPath ? path.resolve(docs, path.dirname(filename), linkPath.split('?')[0]) : path.join(docs, filename);
    if (destination.endsWith(path.sep) || (fs.existsSync(destination) && fs.statSync(destination).isDirectory())) destination = path.join(destination, 'index.html');
    check(destination.startsWith(docs + path.sep), `Link escapes docs: ${value}`);
    check(fs.existsSync(destination), `Missing local target: ${value}`);
    if (fragment && fs.existsSync(destination) && destination.endsWith('.html')) {
      const target = fs.readFileSync(destination, 'utf8');
      check(target.includes(`id="${decodeURIComponent(fragment)}"`), `Missing fragment: ${value}`);
    }
  }
}
const index = JSON.parse(fs.readFileSync(path.join(docs, 'tutorial/search-index.json')));
assert.equal(index.length, pages.length);
for (const term of ['Postgres', 'workspace', 'backup', 'took_from']) assert(index.some(page => page.text.includes(term)), `Missing searchable ${term}`);
execFileSync(process.execPath, ['--check', path.join(docs, 'tutorial/docs.js')]);
if (errors.length) { console.error(errors.join('\n')); process.exit(1); }
console.log(`PASS: ${files.length} pages, ${links} local links/fragments, navigation, metadata, search index, and browser-script syntax.`);
