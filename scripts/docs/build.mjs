import fs from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';
import MarkdownIt from 'markdown-it';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const docs = path.join(root, 'docs');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'scripts/docs/pages.json'), 'utf8'));
const checking = process.argv.includes('--check');
const ids = manifest.groups.flatMap(group => group.pages);
const generated = new Map();
const escape = text => String(text).replace(/[&<>"']/g, char => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
const slug = text => text.toLowerCase().replace(/[^a-z0-9\s-]/g, '').trim().replace(/[\s-]+/g, '-');
const plain = text => text.replace(/<[^>]*>/g, ' ').replace(/[`*#]/g, '').replace(/\[([^\]]+)\]\([^)]*\)/g, '$1').replace(/\s+/g, ' ').trim();
const relative = (from, to) => path.posix.relative(path.posix.dirname(from), to) || path.posix.basename(to);
const pages = new Map(ids.map(id => {
  const source = fs.readFileSync(path.join(docs, `content/${id}.md`), 'utf8');
  const title = source.match(/^# (.+)$/m)?.[1];
  if (!title) throw new Error(`Missing H1: ${id}`);
  const firstParagraph = source.split(/\n\s*\n/).find(part => !part.startsWith('#') && !part.startsWith('```')) || title;
  const description = plain(firstParagraph).slice(0, 200);
  return [id, {id, source, title, description}];
}));

const logo = '<svg viewBox="0 0 36 44" aria-hidden="true"><g fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"><path d="M18 38V9M18 28C18 19 6 24 6 10M18 20C18 12 30 19 30 7"/><circle cx="18" cy="5" r="3"/><circle cx="6" cy="6" r="3"/><circle cx="30" cy="4" r="3"/><circle cx="18" cy="39" r="3"/></g></svg>';
const searchData = [];

function article(page, outputPath) {
  const toc = [];
  const usedIds = new Map();
  const md = new MarkdownIt({html: false, linkify: true, typographer: false});
  md.core.ruler.push('headings', state => {
    for (let i = 0; i < state.tokens.length; i++) {
      const token = state.tokens[i];
      if (token.type !== 'heading_open') continue;
      const text = plain(state.tokens[i + 1].content);
      const base = slug(text) || 'section';
      const count = usedIds.get(base) || 0;
      usedIds.set(base, count + 1);
      const id = base + (count ? `-${count + 1}` : '');
      token.attrSet('id', id);
      if (token.tag === 'h2') toc.push({text, id});
    }
  });
  const defaultLink = md.renderer.rules.link_open || ((tokens, idx, options, env, self) => self.renderToken(tokens, idx, options));
  md.renderer.rules.link_open = (tokens, idx, options, env, self) => {
    let href = tokens[idx].attrGet('href');
    const [base, fragment] = href.split('#');
    const pageId = base.replace(/^\.\//, '').replace(/\.md$/, '');
    if (pages.has(pageId)) href = relative(outputPath, `tutorial/${pageId}.html`) + (fragment ? `#${fragment}` : '');
    else if (/scripts\/team-memory\//.test(base)) href = relative(outputPath, `downloads/team-memory/${base.split('scripts/team-memory/')[1]}`) + (fragment ? `#${fragment}` : '');
    else if (base.startsWith('/tutorial/')) href = relative(outputPath, base.slice(1)) + (fragment ? `#${fragment}` : '');
    else if (base.startsWith('https://github.com/')) tokens[idx].attrSet('rel', 'noopener');
    tokens[idx].attrSet('href', href);
    return defaultLink(tokens, idx, options, env, self);
  };
  const defaultFence = md.renderer.rules.fence;
  md.renderer.rules.fence = (tokens, idx, options, env, self) => {
    const language = tokens[idx].info.trim().split(/\s/)[0] || 'text';
    return `<div class="code-block"><div class="code-toolbar"><span>${escape(language)}</span><button type="button" class="copy-code" aria-label="Copy code block">Copy</button></div>${defaultFence(tokens, idx, options, env, self)}</div>`;
  };
  return {html: md.render(page.source), toc};
}

function render(id, outputPath) {
  const page = pages.get(id);
  const {html, toc} = article(page, outputPath);
  const canonical = `https://deciduous.dev/tutorial/${id === 'index' ? '' : `${id}.html`}`;
  const href = target => escape(relative(outputPath, target));
  const navigation = manifest.groups.map(group => `<section class="nav-group"><h2>${escape(group.title)}</h2><ul>${group.pages.map(item => `<li><a href="${href(`tutorial/${item}.html`)}"${id === item ? ' aria-current="page"' : ''}>${escape(manifest.labels[item])}</a></li>`).join('')}</ul></section>`).join('');
  const position = ids.indexOf(id);
  const adjacent = (target, direction) => target ? `<a href="${href(`tutorial/${target}.html`)}"><span>${direction}</span>${escape(manifest.labels[target])}</a>` : '<span></span>';
  const architecture = id === 'index' ? '<div class="connection-strip" aria-label="Agents connect through HTTP MCP to the shared Deciduous service, which stores memory in Postgres"><span><i class="agent-mark"></i>Your agents</span><span class="connection-label">HTTP MCP</span><span><i class="service-mark"></i>Shared service</span><span class="connection-label">Private network</span><span><i class="database-mark"></i>Postgres</span></div>' : '';
  return `<!doctype html>
<html lang="en" data-docs-generated="true">
<head>
<meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>${escape(page.title)} · Deciduous docs</title>
<meta name="description" content="${escape(page.description)}">
<link rel="canonical" href="${canonical}">
<meta property="og:type" content="article"><meta property="og:site_name" content="Deciduous">
<meta property="og:title" content="${escape(page.title)} · Deciduous docs"><meta property="og:description" content="${escape(page.description)}"><meta property="og:url" content="${canonical}">
<meta property="og:image" content="https://deciduous.dev/og-many-minds.png"><meta property="og:image:width" content="1200"><meta property="og:image:height" content="630"><meta property="og:image:alt" content="Many minds. One memory. Agents connected to a shared Deciduous graph.">
<meta name="twitter:card" content="summary_large_image"><meta name="theme-color" content="#fff0ed">
<link rel="preconnect" href="https://fonts.googleapis.com"><link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=Familjen+Grotesk:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500&display=swap" rel="stylesheet">
<link rel="stylesheet" href="${href('tutorial/style.css')}"><script src="${href('tutorial/docs.js')}" defer></script>
</head>
<body data-search-index="${href('tutorial/search-index.json')}">
<a class="skip-link" href="#content">Skip to content</a>
<header class="docs-header"><a class="brand" href="${href('index.html')}" aria-label="Deciduous home">${logo}<span>deciduous</span></a><a class="docs-label" href="${href('tutorial/index.html')}">Docs</a><div class="header-actions"><button type="button" class="search-open" aria-haspopup="dialog"><span>Search docs</span><kbd>/</kbd></button><a class="github-link" href="https://github.com/notactuallytreyanastasio/deciduous">GitHub</a><button type="button" class="menu-toggle" aria-expanded="false" aria-controls="docs-navigation">Menu</button></div></header>
<div class="docs-layout"><aside class="sidebar" id="docs-navigation"><nav aria-label="Documentation">${navigation}</nav><div class="sidebar-note">Shared server workflow<br><span>Deciduous 1.0</span></div></aside>
<main id="content"><div class="article-meta"><span>${escape(manifest.groups.find(group => group.pages.includes(id)).title)}</span><a href="${href(`content/${id}.md`)}">Read as Markdown</a></div>${architecture}<article>${html}</article><nav class="page-turn" aria-label="Previous and next guide">${adjacent(ids[position - 1], 'Previous')}${adjacent(ids[position + 1], 'Next')}</nav><footer class="article-footer"><a href="${href('index.html')}">Watch the agents work</a><span>Many minds. One memory.</span></footer></main>
<aside class="on-this-page"><nav aria-label="On this page"><h2>On this page</h2><ul>${toc.map(item => `<li><a href="#${escape(item.id)}">${escape(item.text)}</a></li>`).join('')}</ul></nav></aside></div>
<dialog id="docs-search" aria-labelledby="search-title"><form method="dialog"><h2 id="search-title">Search the docs</h2><button aria-label="Close search" class="close-search">Close</button></form><label class="sr-only" for="search-query">Search terms</label><input type="search" id="search-query" placeholder="Try workspace, backup, or two agents" autocomplete="off"><p id="search-status" role="status">Type to search all guides.</p><ul id="search-results"></ul></dialog><div class="sr-only" id="copy-status" role="status"></div>
</body></html>\n`;
}

for (const id of ids) {
  const page = pages.get(id);
  generated.set(`tutorial/${id}.html`, render(id, `tutorial/${id}.html`));
  searchData.push({title: page.title, label: manifest.labels[id], url: `${id}.html`, text: plain(page.source)});
}
for (const [outputPath, id] of Object.entries(manifest.aliases)) generated.set(outputPath, render(id, outputPath));
for (const [outputPath, id] of Object.entries(manifest.markdownAliases)) {
  const markdown = pages.get(id).source.replace(/\]\(([^)#]+\.md)(#[^)]*)?\)/g, (all, name, fragment = '') => pages.has(name.replace('.md', '')) ? `](content/${name}${fragment})` : all);
  generated.set(outputPath, `<!-- Generated from content/${id}.md by scripts/docs/build.mjs. Edit the source. -->\n\n${markdown}`);
}
generated.set('tutorial/search-index.json', JSON.stringify(searchData, null, 2) + '\n');
const scriptRoot = path.join(root, 'scripts/team-memory');
if (fs.existsSync(scriptRoot)) {
  for (const entry of fs.readdirSync(scriptRoot, {withFileTypes: true})) {
    if (entry.isFile() && (/\.(py|ya?ml|md|sh)$/.test(entry.name) || entry.name === 'Caddyfile.example') && !entry.name.startsWith('test')) {
      generated.set(`downloads/team-memory/${entry.name}`, fs.readFileSync(path.join(scriptRoot, entry.name)));
    }
  }
  for (const name of ['Dockerfile', 'compose.yaml', 'README.md', '.gitignore']) {
    generated.set(`downloads/team-memory/postgres-only/${name}`, fs.readFileSync(path.join(scriptRoot, 'postgres-only', name)));
  }
}
let stale = 0;
for (const [outputPath, content] of generated) {
  const filename = path.join(docs, outputPath);
  const bytes = Buffer.from(content);
  if (checking) {
    if (!fs.existsSync(filename) || !fs.readFileSync(filename).equals(bytes)) { console.error(`Out of date: docs/${outputPath}`); stale++; }
  } else {
    fs.mkdirSync(path.dirname(filename), {recursive: true});
    fs.writeFileSync(filename, bytes);
  }
}
if (stale) process.exit(1);
console.log(`${checking ? 'Checked' : 'Built'} ${ids.length} guides, ${Object.keys(manifest.aliases).length} compatible routes, and ${generated.size} generated files.`);
