(() => {
  'use strict';
  const menu = document.querySelector('.menu-toggle');
  menu.addEventListener('click', () => {
    const open = menu.getAttribute('aria-expanded') !== 'true';
    menu.setAttribute('aria-expanded', String(open));
    document.body.classList.toggle('menu-open', open);
  });
  document.querySelectorAll('.copy-code').forEach(button => button.addEventListener('click', async () => {
    const text = button.closest('.code-block').querySelector('code').textContent;
    try {
      await navigator.clipboard.writeText(text);
      button.textContent = 'Copied';
      document.querySelector('#copy-status').textContent = 'Code copied to clipboard.';
      setTimeout(() => { button.textContent = 'Copy'; }, 1600);
    } catch {
      const range = document.createRange();
      range.selectNodeContents(button.closest('.code-block').querySelector('code'));
      const selection = window.getSelection();
      selection.removeAllRanges(); selection.addRange(range);
      document.querySelector('#copy-status').textContent = 'Code selected. Use your keyboard to copy.';
    }
  }));
  const dialog = document.querySelector('#docs-search');
  const query = document.querySelector('#search-query');
  const results = document.querySelector('#search-results');
  const status = document.querySelector('#search-status');
  let index;
  let loading;
  const searchURL = new URL(document.body.dataset.searchIndex, window.location.href);
  const render = () => {
    results.replaceChildren();
    if (!index) return;
    const words = query.value.toLowerCase().trim().split(/\s+/).filter(Boolean);
    if (!words.length) { status.textContent = 'Type to search all guides.'; return; }
    const matches = index.map(page => ({...page, score: words.reduce((sum, word) => sum + (page.title.toLowerCase().includes(word) ? 8 : 0) + (page.text.toLowerCase().includes(word) ? 1 : 0), 0)})).filter(page => words.every(word => `${page.title} ${page.text}`.toLowerCase().includes(word))).sort((a, b) => b.score - a.score).slice(0, 10);
    status.textContent = matches.length ? `${matches.length} matching guides.` : 'No guides match. Try a command name or a shorter phrase.';
    for (const page of matches) {
      const item = document.createElement('li');
      const link = document.createElement('a');
      link.href = new URL(page.url, searchURL).href;
      const title = document.createElement('strong'); title.textContent = page.title;
      const summary = document.createElement('span');
      const offset = Math.max(0, page.text.toLowerCase().indexOf(words[0]) - 50);
      summary.textContent = `${offset ? '…' : ''}${page.text.slice(offset, offset + 180)}…`;
      link.append(title, summary); item.append(link); results.append(item);
    }
  };
  const openSearch = async () => {
    if (!dialog.open) dialog.showModal();
    query.focus();
    if (!index) {
      status.textContent = 'Loading guides…';
      try {
        loading ||= fetch(searchURL).then(response => { if (!response.ok) throw new Error('Search unavailable'); return response.json(); });
        index = await loading;
      } catch {
        loading = undefined;
        status.textContent = 'Search could not load. Use the guide navigation or try again.';
        return;
      }
    }
    render();
  };
  document.querySelector('.search-open').addEventListener('click', openSearch);
  query.addEventListener('input', render);
  document.addEventListener('keydown', event => {
    if ((event.key === '/' || ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k')) && !/INPUT|TEXTAREA|SELECT/.test(event.target.tagName) && !event.target.isContentEditable) { event.preventDefault(); openSearch(); }
  });
  dialog.addEventListener('click', event => { if (event.target === dialog) { const r = dialog.getBoundingClientRect(); if (event.clientX < r.left || event.clientX > r.right || event.clientY < r.top || event.clientY > r.bottom) dialog.close(); } });
})();
