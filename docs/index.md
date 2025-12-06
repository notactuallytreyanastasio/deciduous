---
layout: default
title: Home
---

<style>
.hero-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 40px;
  margin-bottom: 30px;
}
@media (max-width: 768px) {
  .hero-grid { grid-template-columns: 1fr; }
}
.hero-left h1 { margin-top: 0; }
.hero-left img { max-width: 100%; border-radius: 8px; margin: 15px 0; }
.btn-primary {
  display: inline-block;
  background: #3b82f6;
  color: white !important;
  padding: 12px 24px;
  border-radius: 6px;
  text-decoration: none;
  font-weight: 600;
  margin-right: 10px;
  margin-bottom: 10px;
}
.btn-secondary {
  display: inline-block;
  background: #1e293b;
  color: #60a5fa !important;
  padding: 12px 24px;
  border-radius: 6px;
  text-decoration: none;
  font-weight: 600;
  border: 1px solid #3b82f6;
}
.btn-primary:hover { background: #2563eb; }
.btn-secondary:hover { background: #334155; }
.verdict-table { width: 100%; margin: 15px 0; }
.verdict-table th, .verdict-table td { padding: 8px 12px; text-align: left; }
.verdict-table th { border-bottom: 2px solid #334155; }
.verdict-table td { border-bottom: 1px solid #1e293b; }
.code-block {
  background: #0d1117;
  padding: 15px;
  border-radius: 6px;
  overflow-x: auto;
  font-family: monospace;
  font-size: 13px;
  margin: 15px 0;
}
.cards {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 15px;
  margin: 20px 0;
}
@media (max-width: 768px) {
  .cards { grid-template-columns: 1fr; }
}
.card {
  background: #16213e;
  padding: 20px;
  border-radius: 8px;
  border: 1px solid #0f3460;
  text-decoration: none;
}
.card:hover { border-color: #3b82f6; }
.card-title { font-weight: 600; margin-bottom: 5px; }
.card-desc { color: #999; font-size: 14px; }
</style>

<div class="hero-grid">
<div class="hero-left">

<h1>Losselot</h1>

<p><strong>Audio forensics meets AI-assisted development.</strong></p>

<p>Detect fake lossless files. Every decision tracked in a queryable graph.</p>

<img src="demo.gif" alt="Losselot Demo">

<p>
<a href="analyzer.html" class="btn-primary">Try in Browser</a>
<a href="demo/" class="btn-secondary">View Decision Graph</a>
</p>

</div>
<div class="hero-right">

<h2>How It Works</h2>

<p>When someone converts MP3 to FLAC, the removed frequencies don't come back:</p>

<ul>
<li><strong>Spectral</strong> - FFT detects frequency cutoffs</li>
<li><strong>Binary</strong> - Finds encoder signatures (LAME, FFmpeg)</li>
<li><strong>Combined</strong> - Agreement increases confidence</li>
</ul>

<table class="verdict-table">
<tr><th>Score</th><th>Verdict</th><th>Meaning</th></tr>
<tr><td>0-34</td><td>OK</td><td>Clean</td></tr>
<tr><td>35-64</td><td>SUSPECT</td><td>Possibly transcoded</td></tr>
<tr><td>65+</td><td>TRANSCODE</td><td>Definitely lossy origin</td></tr>
</table>

<h2>Quick Start</h2>

<div class="code-block">
git clone https://github.com/notactuallytreyanastasio/losselot<br>
cd losselot && cargo build --release<br>
./target/release/losselot serve ~/Music/
</div>

</div>
</div>

<hr>

<h2>The Living Museum</h2>

<p>This project tracks every decision in a queryable graph. When context is lost, the reasoning survives.</p>

<div class="cards">
<a href="decision-graph" class="card">
<div class="card-title" style="color: #4ade80;">Decision Graph</div>
<div class="card-desc">77+ nodes of dev decisions</div>
</a>
<a href="claude-tooling" class="card">
<div class="card-title" style="color: #60a5fa;">Claude Tooling</div>
<div class="card-desc">AI development workflow</div>
</a>
<a href="story" class="card">
<div class="card-title" style="color: #a855f7;">The Story</div>
<div class="card-desc">How this evolved</div>
</a>
</div>

<p style="text-align: center; margin-top: 30px;">
<a href="https://github.com/notactuallytreyanastasio/losselot">View on GitHub</a>
</p>
