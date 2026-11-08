# Build the documentation

Edit Markdown in `docs/content/`. Navigation labels and compatible old routes live in `scripts/docs/pages.json`. The site is static HTML; it needs no runtime service or JavaScript to read a guide.

```sh
npm ci --prefix scripts/docs
npm run build --prefix scripts/docs
npm run check --prefix scripts/docs
```

Commit generated HTML, the search index, Markdown compatibility pages, and downloadable scripts with the source. CI checks that generated files match. The homepage replay and demo viewer are independent of this generator.

Each Markdown file begins with one H1. Use relative links such as `[Connect clients](clients.md)`; the renderer maps them to the canonical HTML page. Links to `scripts/team-memory/` map to downloadable copies in `docs/downloads/team-memory/`.

Local preview:

```sh
python3 -m http.server 8766 --directory docs --bind 127.0.0.1
```

Open `http://127.0.0.1:8766/tutorial/`. Check desktop and narrow layouts, keyboard navigation, code copying, and search before publishing. Keep the server bound to loopback.
