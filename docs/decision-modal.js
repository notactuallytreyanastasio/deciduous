// Decision Modal Component - shows decision chain for a feature
// Usage: <button onclick="DecisionModal.show({title: 'Feature Name', nodeIds: [1,2,3]})">View Decisions</button>
// Or: <button onclick="DecisionModal.show({title: 'Feature Name', filter: n => n.title.includes('WASM')})">View Decisions</button>

const DecisionModal = (function() {
    let graphData = null;
    let modalEl = null;

    const styles = `
        .decision-modal-overlay {
            position: fixed; top: 0; left: 0; right: 0; bottom: 0;
            background: rgba(0,0,0,0.8); z-index: 10000;
            display: flex; align-items: center; justify-content: center;
            opacity: 0; transition: opacity 0.2s;
        }
        .decision-modal-overlay.visible { opacity: 1; }
        .decision-modal {
            background: #1a1a2e; border-radius: 12px; max-width: 700px; width: 90%;
            max-height: 80vh; overflow: hidden; display: flex; flex-direction: column;
            border: 1px solid #0f3460; box-shadow: 0 20px 60px rgba(0,0,0,0.5);
        }
        .decision-modal-header {
            padding: 20px; border-bottom: 1px solid #0f3460;
            display: flex; align-items: center; gap: 15px;
        }
        .decision-modal-header h2 { flex: 1; margin: 0; font-size: 18px; color: #eee; }
        .decision-modal-close {
            background: none; border: none; color: #666; font-size: 24px;
            cursor: pointer; padding: 0; line-height: 1;
        }
        .decision-modal-close:hover { color: #eee; }
        .decision-modal-body { padding: 20px; overflow-y: auto; flex: 1; }
        .decision-modal-empty { text-align: center; color: #666; padding: 40px; }
        .dm-timeline { position: relative; padding-left: 25px; }
        .dm-timeline::before {
            content: ''; position: absolute; left: 6px; top: 0; bottom: 0;
            width: 2px; background: linear-gradient(to bottom, #0f3460, #00d9ff, #0f3460);
        }
        .dm-node {
            position: relative; margin-bottom: 16px; padding: 12px;
            background: #16213e; border-radius: 8px; border: 1px solid #0f3460;
        }
        .dm-node::before {
            content: ''; position: absolute; left: -23px; top: 16px;
            width: 10px; height: 10px; border-radius: 50%;
            border: 2px solid #00d9ff; background: #1a1a2e;
        }
        .dm-node.type-goal::before { background: #22c55e; border-color: #22c55e; }
        .dm-node.type-decision::before { background: #eab308; border-color: #eab308; }
        .dm-node.type-action::before { background: #ef4444; border-color: #ef4444; }
        .dm-node.type-outcome::before { background: #a855f7; border-color: #a855f7; }
        .dm-node.type-option::before { background: #06b6d4; border-color: #06b6d4; }
        .dm-node.type-observation::before { background: #6b7280; border-color: #6b7280; }
        .dm-node-header { display: flex; align-items: center; gap: 8px; margin-bottom: 6px; }
        .dm-type {
            font-size: 9px; text-transform: uppercase; padding: 2px 6px;
            border-radius: 3px; font-weight: 600;
        }
        .dm-type.type-goal { background: #22c55e; color: #000; }
        .dm-type.type-decision { background: #eab308; color: #000; }
        .dm-type.type-option { background: #06b6d4; color: #000; }
        .dm-type.type-action { background: #ef4444; color: #fff; }
        .dm-type.type-outcome { background: #a855f7; color: #fff; }
        .dm-type.type-observation { background: #6b7280; color: #fff; }
        .dm-confidence {
            font-size: 9px; padding: 2px 6px; border-radius: 10px; font-weight: 600;
        }
        .dm-confidence.high { background: #22c55e33; color: #4ade80; }
        .dm-confidence.med { background: #eab30833; color: #fbbf24; }
        .dm-confidence.low { background: #ef444433; color: #f87171; }
        .dm-title { font-size: 13px; color: #eee; flex: 1; }
        .dm-time { font-size: 10px; color: #666; }
        .dm-desc { font-size: 12px; color: #999; line-height: 1.4; }
        .dm-edge { font-size: 10px; color: #4ade80; margin: -8px 0 8px 0; font-weight: 500; }
        .dm-footer { padding: 15px 20px; border-top: 1px solid #0f3460; text-align: center; }
        .dm-footer a { color: #00d9ff; text-decoration: none; font-size: 13px; }
        .dm-footer a:hover { text-decoration: underline; }
    `;

    function injectStyles() {
        if (document.getElementById('decision-modal-styles')) return;
        const style = document.createElement('style');
        style.id = 'decision-modal-styles';
        style.textContent = styles;
        document.head.appendChild(style);
    }

    async function loadGraph() {
        if (graphData) return graphData;
        try {
            const base = document.querySelector('script[src*="decision-modal"]')?.src.replace('decision-modal.js', '') || './';
            const res = await fetch(base + 'demo/graph-data.json');
            graphData = await res.json();
            return graphData;
        } catch (e) {
            console.error('Failed to load decision graph:', e);
            return null;
        }
    }

    function getConfidence(node) {
        if (!node.metadata_json) return null;
        try { return JSON.parse(node.metadata_json).confidence; } catch { return null; }
    }

    function confidenceBadge(conf) {
        if (conf === null || conf === undefined) return '';
        const level = conf >= 70 ? 'high' : conf >= 40 ? 'med' : 'low';
        return `<span class="dm-confidence ${level}">${conf}%</span>`;
    }

    function renderNodes(nodes, edges) {
        if (!nodes.length) return '<div class="decision-modal-empty">No decisions found</div>';

        const edgeMap = {};
        edges.forEach(e => { edgeMap[e.to_node_id] = e; });

        return `<div class="dm-timeline">${nodes.map(node => {
            const conf = getConfidence(node);
            const edge = edgeMap[node.id];
            const time = new Date(node.created_at).toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit' });
            return `
                ${edge?.rationale ? `<div class="dm-edge">↳ ${edge.rationale}</div>` : ''}
                <div class="dm-node type-${node.node_type}">
                    <div class="dm-node-header">
                        <span class="dm-type type-${node.node_type}">${node.node_type}</span>
                        ${confidenceBadge(conf)}
                        <span class="dm-title">${node.title}</span>
                        <span class="dm-time">${time}</span>
                    </div>
                    ${node.description ? `<div class="dm-desc">${node.description}</div>` : ''}
                </div>
            `;
        }).join('')}</div>`;
    }

    function createModal() {
        if (modalEl) return modalEl;
        modalEl = document.createElement('div');
        modalEl.className = 'decision-modal-overlay';
        modalEl.innerHTML = `
            <div class="decision-modal">
                <div class="decision-modal-header">
                    <h2 id="dm-title">Decision Chain</h2>
                    <button class="decision-modal-close" onclick="DecisionModal.hide()">&times;</button>
                </div>
                <div class="decision-modal-body" id="dm-body"></div>
                <div class="dm-footer">
                    <a href="demo/" target="_blank">View full decision graph →</a>
                </div>
            </div>
        `;
        modalEl.addEventListener('click', e => { if (e.target === modalEl) DecisionModal.hide(); });
        document.body.appendChild(modalEl);
        return modalEl;
    }

    return {
        async show(options = {}) {
            injectStyles();
            const modal = createModal();
            const data = await loadGraph();

            document.getElementById('dm-title').textContent = options.title || 'Decision Chain';

            if (!data) {
                document.getElementById('dm-body').innerHTML = '<div class="decision-modal-empty">Failed to load decision graph</div>';
            } else {
                let nodes = data.nodes;
                let edges = data.edges;

                // Filter nodes
                if (options.nodeIds) {
                    const ids = new Set(options.nodeIds);
                    nodes = nodes.filter(n => ids.has(n.id));
                } else if (options.filter) {
                    nodes = nodes.filter(options.filter);
                } else if (options.search) {
                    const term = options.search.toLowerCase();
                    nodes = nodes.filter(n => n.title.toLowerCase().includes(term) || (n.description || '').toLowerCase().includes(term));
                }

                // Sort by creation time
                nodes.sort((a, b) => new Date(a.created_at) - new Date(b.created_at));

                // Filter edges to only those between our nodes
                const nodeIds = new Set(nodes.map(n => n.id));
                edges = edges.filter(e => nodeIds.has(e.from_node_id) && nodeIds.has(e.to_node_id));

                document.getElementById('dm-body').innerHTML = renderNodes(nodes, edges);
            }

            requestAnimationFrame(() => modal.classList.add('visible'));
        },

        hide() {
            if (modalEl) {
                modalEl.classList.remove('visible');
                setTimeout(() => modalEl.remove(), 200);
                modalEl = null;
            }
        }
    };
})();
