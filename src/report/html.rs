//! HTML report generation with D3.js visualizations

use crate::analyzer::{AnalysisResult, Verdict};
use crate::report::Summary;
use std::io::{self, Write};

pub fn write<W: Write>(writer: &mut W, results: &[AnalysisResult]) -> io::Result<()> {
    let summary = Summary::from_results(results);

    // Sort by score descending
    let mut sorted_results: Vec<_> = results.iter().collect();
    sorted_results.sort_by(|a, b| b.combined_score.cmp(&a.combined_score));

    // Build JSON data for D3.js
    let json_data = build_json_data(&sorted_results);

    // Write the full HTML document
    write!(writer, r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Losselot Analysis Report</title>
    <script src="https://d3js.org/d3.v7.min.js"></script>
    <style>
        :root {{
            --bg: #f5f5f7;
            --card: #ffffff;
            --border: #d2d2d7;
            --text: #1d1d1f;
            --dim: #86868b;
            --ok: #34c759;
            --suspect: #ff9f0a;
            --transcode: #ff3b30;
            --error: #8e8e93;
            --accent: #007aff;
            --shadow: 0 2px 8px rgba(0,0,0,0.08), 0 1px 2px rgba(0,0,0,0.04);
            --shadow-hover: 0 4px 16px rgba(0,0,0,0.12), 0 2px 4px rgba(0,0,0,0.06);
        }}
        * {{ box-sizing: border-box; margin: 0; padding: 0; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'SF Pro Display', 'SF Pro Text', 'Helvetica Neue', Helvetica, Arial, sans-serif;
            background: var(--bg);
            color: var(--text);
            line-height: 1.5;
            -webkit-font-smoothing: antialiased;
            -moz-osx-font-smoothing: grayscale;
        }}
        .container {{ max-width: 1400px; margin: 0 auto; padding: 3rem 2rem; }}

        /* Header */
        .header {{
            display: flex;
            align-items: center;
            gap: 1rem;
            margin-bottom: 2.5rem;
            padding-bottom: 1.5rem;
            border-bottom: 1px solid var(--border);
        }}
        .logo {{
            font-size: 2.25rem;
            font-weight: 700;
            letter-spacing: -0.02em;
            background: linear-gradient(135deg, #007aff 0%, #5856d6 50%, #af52de 100%);
            -webkit-background-clip: text;
            -webkit-text-fill-color: transparent;
            background-clip: text;
        }}
        .subtitle {{ color: var(--dim); font-size: 0.9375rem; font-weight: 400; letter-spacing: -0.01em; }}

        /* Stats Row */
        .stats {{
            display: grid;
            grid-template-columns: repeat(4, 1fr);
            gap: 1.25rem;
            margin-bottom: 2.5rem;
        }}
        .stat {{
            background: var(--card);
            border-radius: 16px;
            padding: 1.75rem;
            text-align: center;
            box-shadow: var(--shadow);
            transition: transform 0.2s ease, box-shadow 0.2s ease;
        }}
        .stat:hover {{
            transform: translateY(-2px);
            box-shadow: var(--shadow-hover);
        }}
        .stat-value {{ font-size: 2.75rem; font-weight: 600; line-height: 1; letter-spacing: -0.02em; }}
        .stat-label {{ color: var(--dim); font-size: 0.8125rem; font-weight: 500; text-transform: uppercase; letter-spacing: 0.04em; margin-top: 0.5rem; }}
        .stat.ok .stat-value {{ color: var(--ok); }}
        .stat.suspect .stat-value {{ color: var(--suspect); }}
        .stat.transcode .stat-value {{ color: var(--transcode); }}

        /* Charts Grid */
        .charts {{
            display: grid;
            grid-template-columns: 320px 1fr;
            gap: 1.5rem;
            margin-bottom: 2.5rem;
        }}
        .chart-card {{
            background: var(--card);
            border-radius: 16px;
            padding: 1.75rem;
            box-shadow: var(--shadow);
            transition: box-shadow 0.2s ease;
        }}
        .chart-card:hover {{
            box-shadow: var(--shadow-hover);
        }}
        .chart-title {{
            font-size: 0.9375rem;
            font-weight: 600;
            margin-bottom: 1.25rem;
            color: var(--text);
            letter-spacing: -0.01em;
        }}
        #donut-chart {{ display: flex; justify-content: center; }}
        #spectrum-chart {{ width: 100%; }}

        /* Donut legend */
        .donut-legend {{
            display: flex;
            justify-content: center;
            gap: 1.5rem;
            margin-top: 1.25rem;
            flex-wrap: wrap;
        }}
        .legend-item {{
            display: flex;
            align-items: center;
            gap: 0.5rem;
            font-size: 0.8125rem;
            font-weight: 500;
            color: var(--dim);
        }}
        .legend-dot {{
            width: 10px;
            height: 10px;
            border-radius: 50%;
        }}

        /* File Details Panel */
        .detail-panel {{
            background: var(--card);
            border-radius: 16px;
            padding: 2rem;
            margin-bottom: 2.5rem;
            display: none;
            box-shadow: var(--shadow-hover);
        }}
        .detail-panel.active {{ display: block; animation: slideIn 0.3s ease; }}
        @keyframes slideIn {{
            from {{ opacity: 0; transform: translateY(-10px); }}
            to {{ opacity: 1; transform: translateY(0); }}
        }}
        .detail-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 1.5rem;
            padding-bottom: 1rem;
            border-bottom: 1px solid var(--border);
        }}
        .detail-filename {{
            font-family: 'SF Mono', 'Menlo', 'Monaco', monospace;
            font-size: 1rem;
            font-weight: 500;
            color: var(--accent);
        }}
        .detail-close {{
            background: rgba(0,0,0,0.05);
            border: none;
            color: var(--dim);
            cursor: pointer;
            font-size: 1.25rem;
            padding: 0.5rem 0.75rem;
            border-radius: 8px;
            transition: all 0.15s ease;
        }}
        .detail-close:hover {{ background: rgba(0,0,0,0.1); color: var(--text); }}
        .detail-grid {{
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 1.5rem;
        }}
        #file-spectrum {{ width: 100%; }}

        /* Table */
        .table-container {{
            background: var(--card);
            border-radius: 16px;
            overflow: hidden;
            box-shadow: var(--shadow);
        }}
        table {{
            width: 100%;
            border-collapse: collapse;
        }}
        th, td {{ padding: 1rem 1.25rem; text-align: left; }}
        th {{
            background: rgba(0,0,0,0.02);
            font-weight: 600;
            font-size: 0.6875rem;
            text-transform: uppercase;
            letter-spacing: 0.06em;
            color: var(--dim);
            border-bottom: 1px solid var(--border);
        }}
        tr {{ cursor: pointer; transition: background 0.15s ease; }}
        tr:hover td {{ background: rgba(0,122,255,0.04); }}
        tr.selected td {{ background: rgba(0,122,255,0.08); }}
        td {{ border-bottom: 1px solid rgba(0,0,0,0.06); }}
        tr:last-child td {{ border-bottom: none; }}

        .verdict {{
            display: inline-flex;
            align-items: center;
            gap: 0.375rem;
            padding: 0.3125rem 0.625rem;
            border-radius: 6px;
            font-size: 0.6875rem;
            font-weight: 600;
            text-transform: uppercase;
            letter-spacing: 0.02em;
        }}
        .verdict.ok {{ background: rgba(52,199,89,0.12); color: #1d8348; }}
        .verdict.suspect {{ background: rgba(255,159,10,0.12); color: #b36b00; }}
        .verdict.transcode {{ background: rgba(255,59,48,0.12); color: #c9302c; }}
        .verdict.error {{ background: rgba(142,142,147,0.12); color: var(--error); }}

        .score-cell {{ display: flex; align-items: center; gap: 0.75rem; font-weight: 500; }}
        .score-bar {{
            width: 70px;
            height: 6px;
            background: rgba(0,0,0,0.08);
            border-radius: 3px;
            overflow: hidden;
        }}
        .score-fill {{ height: 100%; border-radius: 3px; transition: width 0.3s ease; }}
        .score-fill.low {{ background: linear-gradient(90deg, #34c759, #30d158); }}
        .score-fill.medium {{ background: linear-gradient(90deg, #ff9f0a, #ffb340); }}
        .score-fill.high {{ background: linear-gradient(90deg, #ff3b30, #ff6961); }}

        .flags {{ display: flex; flex-wrap: wrap; gap: 0.375rem; }}
        .flag {{
            background: rgba(0,0,0,0.05);
            padding: 0.25rem 0.5rem;
            border-radius: 4px;
            font-size: 0.6875rem;
            font-family: 'SF Mono', 'Menlo', monospace;
            color: var(--dim);
            font-weight: 500;
        }}
        .filepath {{
            max-width: 220px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            font-family: 'SF Mono', 'Menlo', monospace;
            font-size: 0.8125rem;
            color: var(--dim);
        }}
        .mono {{ font-family: 'SF Mono', 'Menlo', monospace; font-size: 0.8125rem; font-weight: 500; }}
        .dim {{ color: var(--dim); }}

        /* Spectrum bars */
        .bar-ok {{ fill: var(--ok); }}
        .bar-warning {{ fill: var(--suspect); }}
        .bar-danger {{ fill: var(--transcode); }}

        /* Tooltip */
        .tooltip {{
            position: absolute;
            background: #1d1d1f;
            color: #ffffff;
            border-radius: 8px;
            padding: 0.625rem 0.875rem;
            font-size: 0.8125rem;
            font-weight: 500;
            pointer-events: none;
            opacity: 0;
            transition: opacity 0.15s ease;
            z-index: 1000;
            box-shadow: 0 4px 20px rgba(0,0,0,0.25);
            max-width: 300px;
        }}
        .tooltip.visible {{ opacity: 1; }}
        .tooltip div {{ line-height: 1.4; }}

        /* Footer */
        .footer {{
            margin-top: 3rem;
            padding-top: 1.5rem;
            border-top: 1px solid var(--border);
            color: var(--dim);
            font-size: 0.8125rem;
            text-align: center;
        }}
        .footer a {{ color: var(--accent); text-decoration: none; font-weight: 500; }}
        .footer a:hover {{ text-decoration: underline; }}

        /* Spectral Waterfall Section */
        .waterfall-section {{
            margin-bottom: 2.5rem;
        }}
        .waterfall-card {{
            background: var(--card);
            border-radius: 16px;
            padding: 1.75rem;
            box-shadow: var(--shadow);
        }}
        .waterfall-header {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            margin-bottom: 1.25rem;
        }}
        .waterfall-legend {{
            display: flex;
            align-items: center;
            gap: 0.625rem;
            font-size: 0.75rem;
            color: var(--dim);
            font-weight: 500;
        }}
        .gradient-bar {{
            width: 140px;
            height: 10px;
            border-radius: 5px;
            background: linear-gradient(to right, #e8f4fc, #b3d9f2, #7ec8e3, #5e9ece, #3d7ab5, #1e5799);
        }}
        #waterfall-chart {{
            width: 100%;
            overflow-x: auto;
        }}
        .waterfall-cell {{
            cursor: pointer;
            transition: all 0.15s ease;
        }}
        .waterfall-cell:hover {{
            stroke: var(--accent);
            stroke-width: 2;
        }}
        .waterfall-cell.highlighted {{
            stroke: var(--text);
            stroke-width: 2;
        }}
        .freq-label {{
            font-size: 0.6875rem;
            fill: var(--dim);
            font-weight: 500;
        }}
        .file-label {{
            font-size: 0.6875rem;
            fill: var(--dim);
            cursor: pointer;
            font-weight: 500;
        }}
        .file-label:hover {{
            fill: var(--accent);
        }}
        .file-label.suspect {{
            fill: #b36b00;
        }}
        .file-label.transcode {{
            fill: #c9302c;
        }}

        /* Enhanced Detail Panel */
        .detail-grid-enhanced {{
            display: grid;
            grid-template-columns: 1fr;
            gap: 1.5rem;
        }}
        .spectrum-analyzer {{
            background: linear-gradient(180deg, #fafbfc 0%, #f0f2f5 100%);
            border-radius: 12px;
            padding: 1.25rem;
            position: relative;
            border: 1px solid rgba(0,0,0,0.06);
        }}
        .spectrum-analyzer-label {{
            position: absolute;
            bottom: 10px;
            left: 50%;
            transform: translateX(-50%);
            font-size: 0.625rem;
            color: var(--dim);
            letter-spacing: 0.12em;
            font-weight: 600;
            text-transform: uppercase;
        }}
        #freq-response-curve {{
            width: 100%;
        }}
        .freq-band-highlight {{
            fill: rgba(255, 59, 48, 0.12);
            stroke: var(--transcode);
            stroke-width: 1;
            stroke-dasharray: 4, 2;
        }}
        .freq-band-ok {{
            fill: rgba(52, 199, 89, 0.08);
        }}
        .drop-annotation {{
            font-size: 0.6875rem;
            fill: #c9302c;
            font-weight: 700;
        }}
        .curve-path {{
            fill: none;
            stroke-width: 2.5;
            stroke-linecap: round;
        }}
        .curve-gradient {{
            fill: url(#curveGradient);
            opacity: 0.4;
        }}
        .freq-marker {{
            stroke: rgba(0,0,0,0.15);
            stroke-dasharray: 3, 3;
            stroke-width: 1;
        }}
        .freq-marker-label {{
            font-size: 0.625rem;
            fill: var(--dim);
            font-weight: 500;
        }}

        /* Spectrum bars animation */
        .spectrum-bar {{
            transition: height 0.3s ease, fill 0.3s ease;
        }}
        .spectrum-bar.animating {{
            animation: pulse 0.5s ease-in-out;
        }}
        @keyframes pulse {{
            0%, 100% {{ opacity: 1; }}
            50% {{ opacity: 0.7; }}
        }}

        /* Problem indicator badges */
        .problem-badge {{
            display: inline-flex;
            align-items: center;
            gap: 0.25rem;
            padding: 0.25rem 0.5rem;
            border-radius: 5px;
            font-size: 0.6875rem;
            font-weight: 600;
            background: rgba(255, 59, 48, 0.1);
            color: #c9302c;
            margin-right: 0.25rem;
        }}
        .problem-badge.warning {{
            background: rgba(255, 159, 10, 0.1);
            color: #b36b00;
        }}
        .problem-badge svg {{
            width: 12px;
            height: 12px;
        }}

        /* Clickable frequency bands in waterfall */
        .band-clickable {{
            cursor: pointer;
        }}
        .band-clickable:hover {{
            filter: brightness(0.95);
        }}

        /* SVG axis styling for light theme */
        .grid line {{ stroke: rgba(0,0,0,0.08); }}
        .grid path {{ stroke: none; }}
    </style>
</head>
<body>
    <div class="container">
        <div class="header">
            <div>
                <div class="logo">Losselot</div>
                <div class="subtitle">Audio Transcode Detection Report</div>
            </div>
        </div>

        <div class="stats">
            <div class="stat ok">
                <div class="stat-value">{ok}</div>
                <div class="stat-label">Clean</div>
            </div>
            <div class="stat suspect">
                <div class="stat-value">{suspect}</div>
                <div class="stat-label">Suspect</div>
            </div>
            <div class="stat transcode">
                <div class="stat-value">{transcode}</div>
                <div class="stat-label">Transcode</div>
            </div>
            <div class="stat">
                <div class="stat-value">{total}</div>
                <div class="stat-label">Total Files</div>
            </div>
        </div>

        <div class="charts">
            <div class="chart-card">
                <div class="chart-title">Verdict Distribution</div>
                <div id="donut-chart"></div>
                <div class="donut-legend">
                    <div class="legend-item"><div class="legend-dot" style="background: var(--ok)"></div>Clean</div>
                    <div class="legend-item"><div class="legend-dot" style="background: var(--suspect)"></div>Suspect</div>
                    <div class="legend-item"><div class="legend-dot" style="background: var(--transcode)"></div>Transcode</div>
                </div>
            </div>
            <div class="chart-card">
                <div class="chart-title">Score Distribution by File</div>
                <div id="spectrum-chart"></div>
            </div>
        </div>

        <div class="waterfall-section">
            <div class="waterfall-card">
                <div class="waterfall-header">
                    <div class="chart-title" style="margin-bottom: 0;">Spectral Waterfall - Frequency Band Analysis</div>
                    <div class="waterfall-legend">
                        <span>Low Energy</span>
                        <div class="gradient-bar"></div>
                        <span>High Energy</span>
                    </div>
                </div>
                <div id="waterfall-chart"></div>
                <div style="margin-top: 0.75rem; font-size: 0.75rem; color: var(--dim);">
                    Click any cell to see detailed analysis. Sharp drops between bands (dark to light transitions) indicate lossy compression artifacts.
                </div>
            </div>
        </div>

        <div class="detail-panel" id="detail-panel">
            <div class="detail-header">
                <div class="detail-filename" id="detail-filename">filename.mp3</div>
                <button class="detail-close" onclick="closeDetail()">&times;</button>
            </div>
            <div class="detail-grid-enhanced">
                <div class="spectrum-analyzer">
                    <div class="chart-title">Frequency Response Curve</div>
                    <div id="freq-response-curve"></div>
                    <div class="spectrum-analyzer-label">FREQUENCY SPECTRUM ANALYSIS</div>
                </div>
                <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 1.5rem;">
                    <div>
                        <div class="chart-title">Frequency Band Energy</div>
                        <div id="file-spectrum"></div>
                    </div>
                    <div>
                        <div class="chart-title">Analysis Details</div>
                        <div id="file-details"></div>
                    </div>
                </div>
            </div>
        </div>

        <div class="table-container">
            <table>
                <thead>
                    <tr>
                        <th>Verdict</th>
                        <th>Score</th>
                        <th>Bitrate</th>
                        <th>Spectral</th>
                        <th>Binary</th>
                        <th>Encoder</th>
                        <th>Flags</th>
                        <th>File</th>
                    </tr>
                </thead>
                <tbody id="results-table">
                </tbody>
            </table>
        </div>

        <div class="footer">
            Generated by <a href="https://github.com/notactuallytreyanastasio/losselot" target="_blank">Losselot</a>
        </div>
    </div>

    <div class="tooltip" id="tooltip"></div>

    <script>
    const data = {json_data};

    const colors = {{
        ok: '#34c759',
        suspect: '#ff9f0a',
        transcode: '#ff3b30',
        error: '#8e8e93'
    }};

    // Donut Chart
    function drawDonutChart() {{
        const width = 280, height = 280;
        const radius = Math.min(width, height) / 2;

        const svg = d3.select('#donut-chart')
            .append('svg')
            .attr('width', width)
            .attr('height', height)
            .append('g')
            .attr('transform', `translate(${{width/2}},${{height/2}})`);

        const pieData = [
            {{ label: 'Clean', value: data.summary.ok, color: colors.ok }},
            {{ label: 'Suspect', value: data.summary.suspect, color: colors.suspect }},
            {{ label: 'Transcode', value: data.summary.transcode, color: colors.transcode }}
        ].filter(d => d.value > 0);

        const pie = d3.pie().value(d => d.value).sort(null);
        const arc = d3.arc().innerRadius(radius * 0.6).outerRadius(radius * 0.9);
        const arcHover = d3.arc().innerRadius(radius * 0.6).outerRadius(radius * 0.95);

        const arcs = svg.selectAll('path')
            .data(pie(pieData))
            .enter()
            .append('path')
            .attr('d', arc)
            .attr('fill', d => d.data.color)
            .attr('stroke', '#ffffff')
            .attr('stroke-width', 3)
            .style('cursor', 'pointer')
            .on('mouseover', function(event, d) {{
                d3.select(this).transition().duration(100).attr('d', arcHover);
                showTooltip(event, `${{d.data.label}}: ${{d.data.value}} files`);
            }})
            .on('mouseout', function() {{
                d3.select(this).transition().duration(100).attr('d', arc);
                hideTooltip();
            }});

        // Center text
        svg.append('text')
            .attr('text-anchor', 'middle')
            .attr('dy', '-0.2em')
            .style('font-size', '2.25rem')
            .style('font-weight', '600')
            .style('fill', '#1d1d1f')
            .style('letter-spacing', '-0.02em')
            .text(data.summary.total);

        svg.append('text')
            .attr('text-anchor', 'middle')
            .attr('dy', '1.5em')
            .style('font-size', '0.8125rem')
            .style('fill', '#86868b')
            .style('font-weight', '500')
            .text('files');
    }}

    // Score Distribution Chart
    function drawScoreChart() {{
        const container = document.getElementById('spectrum-chart');
        const margin = {{ top: 20, right: 30, bottom: 60, left: 50 }};
        const width = container.clientWidth - margin.left - margin.right;
        const height = 300 - margin.top - margin.bottom;

        const svg = d3.select('#spectrum-chart')
            .append('svg')
            .attr('width', width + margin.left + margin.right)
            .attr('height', height + margin.top + margin.bottom)
            .append('g')
            .attr('transform', `translate(${{margin.left}},${{margin.top}})`);

        const x = d3.scaleBand()
            .domain(data.files.map((d, i) => i))
            .range([0, width])
            .padding(0.2);

        const y = d3.scaleLinear()
            .domain([0, 100])
            .range([height, 0]);

        // Grid lines
        svg.append('g')
            .attr('class', 'grid')
            .call(d3.axisLeft(y).tickSize(-width).tickFormat(''))
            .style('stroke-dasharray', '3,3')
            .style('stroke-opacity', 0.12);

        // Threshold lines
        [35, 65].forEach(thresh => {{
            svg.append('line')
                .attr('x1', 0)
                .attr('x2', width)
                .attr('y1', y(thresh))
                .attr('y2', y(thresh))
                .attr('stroke', thresh === 65 ? colors.transcode : colors.suspect)
                .attr('stroke-dasharray', '5,5')
                .attr('stroke-opacity', 0.5);
        }});

        // Bars
        svg.selectAll('.bar')
            .data(data.files)
            .enter()
            .append('rect')
            .attr('class', d => {{
                if (d.score >= 65) return 'bar bar-danger';
                if (d.score >= 35) return 'bar bar-warning';
                return 'bar bar-ok';
            }})
            .attr('x', (d, i) => x(i))
            .attr('width', x.bandwidth())
            .attr('y', d => y(d.score))
            .attr('height', d => height - y(d.score))
            .attr('rx', 3)
            .style('cursor', 'pointer')
            .on('mouseover', function(event, d) {{
                d3.select(this).style('opacity', 0.8);
                showTooltip(event, `${{d.filename}}: ${{d.score}}%`);
            }})
            .on('mouseout', function() {{
                d3.select(this).style('opacity', 1);
                hideTooltip();
            }})
            .on('click', (event, d) => showDetail(d));

        // Y axis
        svg.append('g')
            .call(d3.axisLeft(y).ticks(5).tickFormat(d => d + '%'))
            .style('color', '#86868b')
            .style('font-size', '0.75rem');

        // X axis label
        svg.append('text')
            .attr('x', width / 2)
            .attr('y', height + 45)
            .attr('text-anchor', 'middle')
            .style('fill', '#86868b')
            .style('font-size', '0.8125rem')
            .style('font-weight', '500')
            .text('Files (sorted by score)');
    }}

    // File Detail Spectrum
    function drawFileSpectrum(file) {{
        const container = document.getElementById('file-spectrum');
        container.innerHTML = '';

        if (!file.spectral) return;

        const margin = {{ top: 20, right: 30, bottom: 60, left: 60 }};
        const width = container.clientWidth - margin.left - margin.right;
        const height = 250 - margin.top - margin.bottom;

        const svg = d3.select('#file-spectrum')
            .append('svg')
            .attr('width', width + margin.left + margin.right)
            .attr('height', height + margin.top + margin.bottom)
            .append('g')
            .attr('transform', `translate(${{margin.left}},${{margin.top}})`);

        const bands = [
            {{ label: 'Full', value: file.spectral.rms_full, range: '20Hz-20kHz' }},
            {{ label: 'Mid-High', value: file.spectral.rms_mid_high, range: '10-15kHz' }},
            {{ label: 'High', value: file.spectral.rms_high, range: '15-20kHz' }},
            {{ label: 'Upper', value: file.spectral.rms_upper, range: '17-20kHz' }},
            {{ label: 'Ultrasonic', value: file.spectral.rms_ultrasonic, range: '20-22kHz' }}
        ];

        const x = d3.scaleBand()
            .domain(bands.map(d => d.label))
            .range([0, width])
            .padding(0.3);

        const minVal = Math.min(...bands.map(d => d.value), -80);
        const maxVal = Math.max(...bands.map(d => d.value), 0);

        const y = d3.scaleLinear()
            .domain([minVal - 10, maxVal + 10])
            .range([height, 0]);

        // Grid
        svg.append('g')
            .attr('class', 'grid')
            .call(d3.axisLeft(y).tickSize(-width).tickFormat(''))
            .style('stroke-dasharray', '3,3')
            .style('stroke-opacity', 0.12);

        // Bars
        svg.selectAll('.bar')
            .data(bands)
            .enter()
            .append('rect')
            .attr('x', d => x(d.label))
            .attr('width', x.bandwidth())
            .attr('y', d => d.value >= 0 ? y(d.value) : y(0))
            .attr('height', d => Math.abs(y(d.value) - y(0)))
            .attr('rx', 4)
            .attr('fill', (d, i) => {{
                if (i === 4) return d.value < -60 ? colors.transcode : colors.ok;
                return colors.ok;
            }})
            .on('mouseover', function(event, d) {{
                showTooltip(event, `${{d.range}}: ${{d.value.toFixed(1)}} dB`);
            }})
            .on('mouseout', hideTooltip);

        // Zero line
        svg.append('line')
            .attr('x1', 0)
            .attr('x2', width)
            .attr('y1', y(0))
            .attr('y2', y(0))
            .attr('stroke', '#86868b')
            .attr('stroke-dasharray', '3,3');

        // Axes
        svg.append('g')
            .attr('transform', `translate(0,${{height}})`)
            .call(d3.axisBottom(x))
            .style('color', '#86868b')
            .style('font-size', '0.75rem');

        svg.append('g')
            .call(d3.axisLeft(y).ticks(6).tickFormat(d => d + ' dB'))
            .style('color', '#86868b')
            .style('font-size', '0.75rem');
    }}

    // Spectral Waterfall Heatmap
    function drawSpectralWaterfall() {{
        const container = document.getElementById('waterfall-chart');
        const filesWithSpectral = data.files.filter(f => f.spectral);

        if (filesWithSpectral.length === 0) {{
            container.innerHTML = '<div style="text-align: center; color: var(--dim); padding: 2rem;">No spectral data available</div>';
            return;
        }}

        const bandLabels = ['Full\\n20Hz-20k', 'Mid-High\\n10-15kHz', 'High\\n15-20kHz', 'Upper\\n17-20kHz', 'Ultrasonic\\n20-22kHz'];
        const bandKeys = ['rms_full', 'rms_mid_high', 'rms_high', 'rms_upper', 'rms_ultrasonic'];

        const margin = {{ top: 50, right: 30, bottom: 20, left: 200 }};
        const cellWidth = 80;
        const cellHeight = 28;
        const width = bandLabels.length * cellWidth;
        const height = Math.min(filesWithSpectral.length * cellHeight, 600);

        const svg = d3.select('#waterfall-chart')
            .append('svg')
            .attr('width', width + margin.left + margin.right)
            .attr('height', height + margin.top + margin.bottom);

        const g = svg.append('g')
            .attr('transform', `translate(${{margin.left}},${{margin.top}})`);

        // Calculate actual data range for color scale
        let minVal = Infinity, maxVal = -Infinity;
        filesWithSpectral.forEach(f => {{
            bandKeys.forEach(key => {{
                const val = f.spectral[key];
                if (val < minVal) minVal = val;
                if (val > maxVal) maxVal = val;
            }});
        }});

        // Color scale: light blue (low energy) to deep blue (high energy) - Apple style
        const colorScale = d3.scaleSequential()
            .domain([minVal, maxVal])
            .interpolator(d3.interpolateRgbBasis(['#f0f7ff', '#c7e0f4', '#86c1e8', '#4ba3db', '#1a7dc4', '#0055aa']));

        // Create cells
        const displayFiles = filesWithSpectral.slice(0, Math.floor(600 / cellHeight));

        displayFiles.forEach((file, fileIdx) => {{
            const y = fileIdx * cellHeight;

            bandKeys.forEach((key, bandIdx) => {{
                const value = file.spectral[key];
                const x = bandIdx * cellWidth;

                // Determine if this is a "problem" cell
                let isProblem = false;
                if (bandIdx >= 3 && file.spectral.upper_drop > 15) isProblem = true;
                if (bandIdx === 4 && file.spectral.ultrasonic_drop > 25) isProblem = true;

                const cell = g.append('rect')
                    .attr('class', 'waterfall-cell')
                    .attr('x', x + 2)
                    .attr('y', y + 2)
                    .attr('width', cellWidth - 4)
                    .attr('height', cellHeight - 4)
                    .attr('rx', 4)
                    .attr('fill', colorScale(value))
                    .attr('data-file', file.filename)
                    .attr('data-band', bandIdx)
                    .on('mouseover', function(event) {{
                        d3.select(this).classed('highlighted', true);
                        const bandName = bandLabels[bandIdx].replace('\\n', ' ');
                        let tooltipText = `${{file.filename}}\\n${{bandName}}: ${{value.toFixed(1)}} dB`;
                        if (bandIdx >= 3 && file.spectral.upper_drop > 15) {{
                            tooltipText += `\\nUpper Drop: ${{file.spectral.upper_drop.toFixed(1)}} dB`;
                        }}
                        if (bandIdx === 4 && file.spectral.ultrasonic_drop > 25) {{
                            tooltipText += `\\nUltrasonic Drop: ${{file.spectral.ultrasonic_drop.toFixed(1)}} dB`;
                        }}
                        showTooltipMultiline(event, tooltipText);
                    }})
                    .on('mouseout', function() {{
                        d3.select(this).classed('highlighted', false);
                        hideTooltip();
                    }})
                    .on('click', () => showDetail(file));

                // Add warning indicator for problem cells
                if (isProblem) {{
                    g.append('circle')
                        .attr('cx', x + cellWidth - 10)
                        .attr('cy', y + 10)
                        .attr('r', 4)
                        .attr('fill', file.verdict === 'Transcode' ? colors.transcode : colors.suspect)
                        .style('pointer-events', 'none');
                }}
            }});

            // File labels on the left
            g.append('text')
                .attr('class', `file-label ${{file.verdict.toLowerCase()}}`)
                .attr('x', -10)
                .attr('y', y + cellHeight / 2 + 4)
                .attr('text-anchor', 'end')
                .text(file.filename.length > 28 ? file.filename.slice(0, 25) + '...' : file.filename)
                .on('click', () => showDetail(file))
                .append('title')
                .text(file.filename);
        }});

        // Band labels on top
        bandLabels.forEach((label, i) => {{
            const lines = label.split('\\n');
            const textGroup = g.append('g')
                .attr('transform', `translate(${{i * cellWidth + cellWidth/2}}, -10)`);

            lines.forEach((line, lineIdx) => {{
                textGroup.append('text')
                    .attr('class', 'freq-label')
                    .attr('text-anchor', 'middle')
                    .attr('y', lineIdx * 12 - 15)
                    .text(line);
            }});
        }});

        // Add drop indicators between bands
        displayFiles.forEach((file, fileIdx) => {{
            if (!file.spectral) return;
            const y = fileIdx * cellHeight;

            // Upper drop indicator (between High and Upper)
            if (file.spectral.upper_drop > 10) {{
                const dropColor = file.spectral.upper_drop > 15 ? colors.transcode : colors.suspect;
                g.append('path')
                    .attr('d', `M${{3 * cellWidth - 2}},${{y + cellHeight/2}} L${{3 * cellWidth + 4}},${{y + cellHeight/2}}`)
                    .attr('stroke', dropColor)
                    .attr('stroke-width', 2)
                    .attr('marker-end', 'url(#dropArrow)');
            }}

            // Ultrasonic drop indicator
            if (file.spectral.ultrasonic_drop > 15) {{
                const dropColor = file.spectral.ultrasonic_drop > 25 ? colors.transcode : colors.suspect;
                g.append('path')
                    .attr('d', `M${{4 * cellWidth - 2}},${{y + cellHeight/2}} L${{4 * cellWidth + 4}},${{y + cellHeight/2}}`)
                    .attr('stroke', dropColor)
                    .attr('stroke-width', 2);
            }}
        }});

        // Arrow marker definition
        svg.append('defs').append('marker')
            .attr('id', 'dropArrow')
            .attr('viewBox', '0 -5 10 10')
            .attr('refX', 8)
            .attr('markerWidth', 6)
            .attr('markerHeight', 6)
            .attr('orient', 'auto')
            .append('path')
            .attr('d', 'M0,-5L10,0L0,5')
            .attr('fill', colors.transcode);

        // Show truncation notice if needed
        if (filesWithSpectral.length > displayFiles.length) {{
            container.insertAdjacentHTML('beforeend',
                `<div style="text-align: center; color: var(--dim); padding: 0.5rem; font-size: 0.75rem;">
                    Showing ${{displayFiles.length}} of ${{filesWithSpectral.length}} files. Click on table rows below to see all files.
                </div>`);
        }}
    }}

    // Multiline tooltip helper
    function showTooltipMultiline(event, text) {{
        const tooltip = document.getElementById('tooltip');
        tooltip.innerHTML = text.split('\\n').map(line => `<div>${{line}}</div>`).join('');
        tooltip.classList.add('visible');
        tooltip.style.left = (event.pageX + 10) + 'px';
        tooltip.style.top = (event.pageY - 10) + 'px';
    }}

    // Interactive Frequency Response Curve
    function drawFrequencyResponseCurve(file) {{
        const container = document.getElementById('freq-response-curve');
        container.innerHTML = '';

        if (!file.spectral) {{
            container.innerHTML = '<div style="text-align: center; color: var(--dim); padding: 2rem;">No spectral data available</div>';
            return;
        }}

        const margin = {{ top: 30, right: 40, bottom: 50, left: 60 }};
        const width = container.clientWidth - margin.left - margin.right;
        const height = 280 - margin.top - margin.bottom;

        const svg = d3.select('#freq-response-curve')
            .append('svg')
            .attr('width', width + margin.left + margin.right)
            .attr('height', height + margin.top + margin.bottom);

        // Gradient definition for curve fill
        const defs = svg.append('defs');

        const gradient = defs.append('linearGradient')
            .attr('id', 'curveGradient')
            .attr('x1', '0%')
            .attr('y1', '0%')
            .attr('x2', '0%')
            .attr('y2', '100%');

        gradient.append('stop')
            .attr('offset', '0%')
            .attr('stop-color', file.verdict === 'Ok' ? colors.ok : file.verdict === 'Suspect' ? colors.suspect : colors.transcode)
            .attr('stop-opacity', 0.6);

        gradient.append('stop')
            .attr('offset', '100%')
            .attr('stop-color', file.verdict === 'Ok' ? colors.ok : file.verdict === 'Suspect' ? colors.suspect : colors.transcode)
            .attr('stop-opacity', 0.05);

        // Glow filter for problem areas
        const filter = defs.append('filter')
            .attr('id', 'glow');
        filter.append('feGaussianBlur')
            .attr('stdDeviation', '3')
            .attr('result', 'coloredBlur');
        const feMerge = filter.append('feMerge');
        feMerge.append('feMergeNode').attr('in', 'coloredBlur');
        feMerge.append('feMergeNode').attr('in', 'SourceGraphic');

        const g = svg.append('g')
            .attr('transform', `translate(${{margin.left}},${{margin.top}})`);

        // Frequency points (logarithmic scale feel)
        const freqPoints = [
            {{ freq: 20, label: '20Hz', band: 'full' }},
            {{ freq: 100, label: '100Hz', band: 'full' }},
            {{ freq: 1000, label: '1kHz', band: 'full' }},
            {{ freq: 5000, label: '5kHz', band: 'full' }},
            {{ freq: 10000, label: '10kHz', band: 'mid_high' }},
            {{ freq: 15000, label: '15kHz', band: 'high' }},
            {{ freq: 17000, label: '17kHz', band: 'upper' }},
            {{ freq: 19000, label: '19kHz', band: 'upper' }},
            {{ freq: 20000, label: '20kHz', band: 'ultrasonic' }},
            {{ freq: 22000, label: '22kHz', band: 'ultrasonic' }}
        ];

        // Simulate frequency response based on spectral data
        const s = file.spectral;
        const baseLevel = s.rms_full;

        const curveData = [
            {{ freq: 20, db: baseLevel }},
            {{ freq: 100, db: baseLevel + 2 }},
            {{ freq: 500, db: baseLevel + 3 }},
            {{ freq: 1000, db: baseLevel + 2 }},
            {{ freq: 3000, db: baseLevel + 1 }},
            {{ freq: 5000, db: baseLevel }},
            {{ freq: 8000, db: s.rms_mid_high + 2 }},
            {{ freq: 10000, db: s.rms_mid_high }},
            {{ freq: 12000, db: (s.rms_mid_high + s.rms_high) / 2 }},
            {{ freq: 15000, db: s.rms_high }},
            {{ freq: 16000, db: (s.rms_high + s.rms_upper) / 2 }},
            {{ freq: 17000, db: s.rms_upper }},
            {{ freq: 18000, db: (s.rms_upper + s.rms_ultrasonic) / 2 }},
            {{ freq: 19000, db: s.rms_ultrasonic + 3 }},
            {{ freq: 20000, db: s.rms_ultrasonic }},
            {{ freq: 21000, db: s.rms_ultrasonic - 3 }},
            {{ freq: 22000, db: s.rms_ultrasonic - 6 }}
        ];

        // Scales
        const x = d3.scaleLog()
            .domain([20, 22000])
            .range([0, width]);

        const yMin = Math.min(...curveData.map(d => d.db), -80);
        const yMax = Math.max(...curveData.map(d => d.db), 0);

        const y = d3.scaleLinear()
            .domain([yMin - 10, yMax + 5])
            .range([height, 0]);

        // Grid
        g.append('g')
            .attr('class', 'grid')
            .call(d3.axisLeft(y).tickSize(-width).tickFormat(''))
            .style('stroke-dasharray', '3,4')
            .style('stroke-opacity', 0.12);

        // Highlight problem frequency regions
        if (s.upper_drop > 15) {{
            g.append('rect')
                .attr('class', 'freq-band-highlight')
                .attr('x', x(15000))
                .attr('y', 0)
                .attr('width', x(20000) - x(15000))
                .attr('height', height)
                .style('filter', 'url(#glow)');

            g.append('text')
                .attr('class', 'drop-annotation')
                .attr('x', x(17000))
                .attr('y', 20)
                .attr('text-anchor', 'middle')
                .text(`-${{s.upper_drop.toFixed(0)}}dB DROP`);
        }}

        if (s.ultrasonic_drop > 25) {{
            g.append('rect')
                .attr('class', 'freq-band-highlight')
                .attr('x', x(19000))
                .attr('y', 0)
                .attr('width', x(22000) - x(19000))
                .attr('height', height)
                .style('filter', 'url(#glow)');

            if (s.upper_drop <= 15) {{
                g.append('text')
                    .attr('class', 'drop-annotation')
                    .attr('x', x(20500))
                    .attr('y', 20)
                    .attr('text-anchor', 'middle')
                    .text(`320k CLIFF`);
            }}
        }}

        // Area under curve
        const area = d3.area()
            .x(d => x(d.freq))
            .y0(height)
            .y1(d => y(d.db))
            .curve(d3.curveMonotoneX);

        g.append('path')
            .datum(curveData)
            .attr('class', 'curve-gradient')
            .attr('d', area)
            .attr('fill', 'url(#curveGradient)');

        // Main curve line
        const line = d3.line()
            .x(d => x(d.freq))
            .y(d => y(d.db))
            .curve(d3.curveMonotoneX);

        g.append('path')
            .datum(curveData)
            .attr('class', 'curve-path')
            .attr('d', line)
            .attr('stroke', file.verdict === 'Ok' ? colors.ok : file.verdict === 'Suspect' ? colors.suspect : colors.transcode);

        // Interactive points
        curveData.forEach((point, i) => {{
            const isProblemPoint = (point.freq >= 15000 && s.upper_drop > 15) ||
                                   (point.freq >= 19000 && s.ultrasonic_drop > 25);

            g.append('circle')
                .attr('cx', x(point.freq))
                .attr('cy', y(point.db))
                .attr('r', isProblemPoint ? 6 : 4)
                .attr('fill', isProblemPoint ? colors.transcode : (file.verdict === 'Ok' ? colors.ok : colors.suspect))
                .attr('stroke', '#ffffff')
                .attr('stroke-width', 2)
                .style('cursor', 'pointer')
                .on('mouseover', function(event) {{
                    d3.select(this)
                        .transition()
                        .duration(100)
                        .attr('r', isProblemPoint ? 8 : 6);

                    let tooltipText = `${{formatFreq(point.freq)}}: ${{point.db.toFixed(1)}} dB`;
                    if (point.freq >= 17000 && point.freq < 20000 && s.upper_drop > 15) {{
                        tooltipText += `\\nUpper band severely attenuated`;
                    }}
                    if (point.freq >= 20000 && s.ultrasonic_drop > 25) {{
                        tooltipText += `\\n320kbps MP3 cutoff detected`;
                    }}
                    showTooltipMultiline(event, tooltipText);
                }})
                .on('mouseout', function() {{
                    d3.select(this)
                        .transition()
                        .duration(100)
                        .attr('r', isProblemPoint ? 6 : 4);
                    hideTooltip();
                }});
        }});

        // Frequency markers
        const markers = [100, 1000, 10000, 20000];
        markers.forEach(freq => {{
            g.append('line')
                .attr('class', 'freq-marker')
                .attr('x1', x(freq))
                .attr('x2', x(freq))
                .attr('y1', 0)
                .attr('y2', height);

            g.append('text')
                .attr('class', 'freq-marker-label')
                .attr('x', x(freq))
                .attr('y', height + 15)
                .attr('text-anchor', 'middle')
                .text(formatFreq(freq));
        }});

        // Axes
        g.append('g')
            .attr('transform', `translate(0,${{height}})`)
            .call(d3.axisBottom(x)
                .tickValues([20, 50, 100, 200, 500, 1000, 2000, 5000, 10000, 20000])
                .tickFormat(d => ''))
            .style('color', '#86868b');

        g.append('g')
            .call(d3.axisLeft(y).ticks(6).tickFormat(d => d + ' dB'))
            .style('color', '#86868b')
            .style('font-size', '0.75rem');

        // Axis labels
        svg.append('text')
            .attr('x', margin.left + width / 2)
            .attr('y', height + margin.top + 40)
            .attr('text-anchor', 'middle')
            .style('fill', '#86868b')
            .style('font-size', '0.75rem')
            .style('font-weight', '500')
            .text('Frequency (Hz) - Logarithmic Scale');

        svg.append('text')
            .attr('transform', 'rotate(-90)')
            .attr('x', -(margin.top + height / 2))
            .attr('y', 15)
            .attr('text-anchor', 'middle')
            .style('fill', '#86868b')
            .style('font-size', '0.75rem')
            .style('font-weight', '500')
            .text('Energy Level (dB)');

        // Legend for problem indicators
        if (s.upper_drop > 15 || s.ultrasonic_drop > 25) {{
            const legendG = svg.append('g')
                .attr('transform', `translate(${{margin.left + 10}}, ${{margin.top + 5}})`);

            legendG.append('rect')
                .attr('width', 12)
                .attr('height', 12)
                .attr('rx', 2)
                .attr('fill', 'rgba(248, 81, 73, 0.2)')
                .attr('stroke', colors.transcode);

            legendG.append('text')
                .attr('x', 18)
                .attr('y', 10)
                .style('fill', colors.transcode)
                .style('font-size', '0.7rem')
                .text('Lossy compression damage detected');
        }}
    }}

    function formatFreq(freq) {{
        if (freq >= 1000) return (freq / 1000) + 'kHz';
        return freq + 'Hz';
    }}

    // Show file details
    function showDetail(file) {{
        const panel = document.getElementById('detail-panel');
        panel.classList.add('active');
        document.getElementById('detail-filename').textContent = file.filename;

        drawFrequencyResponseCurve(file);
        drawFileSpectrum(file);

        const detailsHtml = `
            <div style="display: grid; gap: 1rem;">
                <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem;">
                    <div style="color: var(--dim);">Verdict:</div>
                    <div><span class="verdict ${{file.verdict.toLowerCase()}}">${{file.verdict}}</span></div>
                    <div style="color: var(--dim);">Score:</div>
                    <div>${{file.score}}%</div>
                    <div style="color: var(--dim);">Bitrate:</div>
                    <div>${{file.bitrate}} kbps</div>
                    <div style="color: var(--dim);">Encoder:</div>
                    <div style="font-family: monospace;">${{file.encoder || 'Unknown'}}</div>
                    ${{file.lowpass ? `<div style="color: var(--dim);">Lowpass:</div><div>${{file.lowpass}} Hz</div>` : ''}}
                </div>
                ${{file.spectral ? `
                <div style="margin-top: 1rem; padding-top: 1rem; border-top: 1px solid var(--border);">
                    <div style="font-weight: 600; margin-bottom: 0.5rem;">Spectral Analysis</div>
                    <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 0.5rem; font-size: 0.875rem;">
                        <div style="color: var(--dim);">Upper Drop:</div>
                        <div style="color: ${{file.spectral.upper_drop > 15 ? 'var(--transcode)' : 'var(--ok)'}}">${{file.spectral.upper_drop.toFixed(1)}} dB</div>
                        <div style="color: var(--dim);">Ultrasonic Drop:</div>
                        <div style="color: ${{file.spectral.ultrasonic_drop > 25 ? 'var(--transcode)' : 'var(--ok)'}}">${{file.spectral.ultrasonic_drop.toFixed(1)}} dB</div>
                        <div style="color: var(--dim);">Flatness (19-21k):</div>
                        <div style="color: ${{file.spectral.ultrasonic_flatness < 0.3 ? 'var(--transcode)' : 'var(--ok)'}}">${{file.spectral.ultrasonic_flatness.toFixed(3)}}</div>
                    </div>
                </div>
                ` : ''}}
                ${{file.flags.length > 0 ? `
                <div style="margin-top: 1rem; padding-top: 1rem; border-top: 1px solid var(--border);">
                    <div style="font-weight: 600; margin-bottom: 0.5rem;">Flags</div>
                    <div class="flags">${{file.flags.map(f => `<span class="flag">${{f}}</span>`).join('')}}</div>
                </div>
                ` : ''}}
            </div>
        `;
        document.getElementById('file-details').innerHTML = detailsHtml;

        // Highlight table row
        document.querySelectorAll('#results-table tr').forEach(tr => tr.classList.remove('selected'));
        const row = document.querySelector(`#results-table tr[data-file="${{file.filename}}"]`);
        if (row) row.classList.add('selected');
    }}

    function closeDetail() {{
        document.getElementById('detail-panel').classList.remove('active');
        document.querySelectorAll('#results-table tr').forEach(tr => tr.classList.remove('selected'));
    }}

    // Tooltip
    function showTooltip(event, text) {{
        const tooltip = document.getElementById('tooltip');
        tooltip.textContent = text;
        tooltip.classList.add('visible');
        tooltip.style.left = (event.pageX + 10) + 'px';
        tooltip.style.top = (event.pageY - 10) + 'px';
    }}

    function hideTooltip() {{
        document.getElementById('tooltip').classList.remove('visible');
    }}

    // Build table
    function buildTable() {{
        const tbody = document.getElementById('results-table');
        data.files.forEach(file => {{
            const scoreClass = file.score >= 65 ? 'high' : file.score >= 35 ? 'medium' : 'low';
            const flagsHtml = file.flags.length > 0
                ? file.flags.map(f => `<span class="flag">${{f}}</span>`).join('')
                : '<span class="dim">—</span>';

            const tr = document.createElement('tr');
            tr.setAttribute('data-file', file.filename);
            tr.innerHTML = `
                <td><span class="verdict ${{file.verdict.toLowerCase()}}">${{file.verdict}}</span></td>
                <td>
                    <div class="score-cell">
                        <div class="score-bar"><div class="score-fill ${{scoreClass}}" style="width: ${{file.score}}%"></div></div>
                        ${{file.score}}%
                    </div>
                </td>
                <td class="mono">${{file.bitrate}}k</td>
                <td class="dim">${{file.spectral_score}}%</td>
                <td class="dim">${{file.binary_score}}%</td>
                <td class="mono">${{file.encoder || '—'}}</td>
                <td class="flags">${{flagsHtml}}</td>
                <td class="filepath" title="${{file.filepath}}">${{file.filename}}</td>
            `;
            tr.onclick = () => showDetail(file);
            tbody.appendChild(tr);
        }});
    }}

    // Initialize
    drawDonutChart();
    drawScoreChart();
    drawSpectralWaterfall();
    buildTable();

    // Auto-show first problematic file if any
    const firstProblem = data.files.find(f => f.verdict !== 'Ok' && f.spectral);
    if (firstProblem) {{
        setTimeout(() => showDetail(firstProblem), 500);
    }}
    </script>
</body>
</html>
"#,
        ok = summary.ok,
        suspect = summary.suspect,
        transcode = summary.transcode,
        total = summary.total,
        json_data = json_data
    )?;

    Ok(())
}

fn build_json_data(results: &[&AnalysisResult]) -> String {
    let files: Vec<String> = results.iter().map(|r| {
        let spectral = if let Some(ref s) = r.spectral_details {
            format!(r#"{{
                "rms_full": {:.2},
                "rms_mid_high": {:.2},
                "rms_high": {:.2},
                "rms_upper": {:.2},
                "rms_ultrasonic": {:.2},
                "upper_drop": {:.2},
                "ultrasonic_drop": {:.2},
                "ultrasonic_flatness": {:.4}
            }}"#,
                s.rms_full, s.rms_mid_high, s.rms_high, s.rms_upper,
                s.rms_ultrasonic, s.upper_drop, s.ultrasonic_drop, s.ultrasonic_flatness
            )
        } else {
            "null".to_string()
        };

        let flags: Vec<String> = r.flags.iter().map(|f| format!("\"{}\"", f)).collect();

        format!(r#"{{
            "filename": "{}",
            "filepath": "{}",
            "verdict": "{}",
            "score": {},
            "spectral_score": {},
            "binary_score": {},
            "bitrate": {},
            "encoder": "{}",
            "lowpass": {},
            "flags": [{}],
            "spectral": {}
        }}"#,
            json_escape(&r.file_name),
            json_escape(&r.file_path),
            r.verdict,
            r.combined_score,
            r.spectral_score,
            r.binary_score,
            r.bitrate,
            json_escape(&r.encoder),
            r.lowpass.map(|l| l.to_string()).unwrap_or_else(|| "null".to_string()),
            flags.join(","),
            spectral
        )
    }).collect();

    let ok_count = results.iter().filter(|r| r.verdict == Verdict::Ok).count();
    let suspect_count = results.iter().filter(|r| r.verdict == Verdict::Suspect).count();
    let transcode_count = results.iter().filter(|r| r.verdict == Verdict::Transcode).count();

    format!(r#"{{
        "summary": {{
            "total": {},
            "ok": {},
            "suspect": {},
            "transcode": {}
        }},
        "files": [{}]
    }}"#,
        results.len(),
        ok_count,
        suspect_count,
        transcode_count,
        files.join(",")
    )
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}
