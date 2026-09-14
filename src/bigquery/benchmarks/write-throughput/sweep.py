#!/usr/bin/env python3
# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

"""Automated benchmark sweep runner and visual report generator.

Runs the BigQuery Write throughput benchmark N times with randomized configuration
settings (channels, multiplex pool, writers, tables, etc.), records time-series and
summary data, and generates a self-contained interactive HTML dashboard with charts.
"""

import argparse
import csv
import datetime
import json
import os
from pathlib import Path
import random
import re
import subprocess
import sys
import time

# Candidate parameter choices for random exploration
CHANNELS = [1, 2, 4, 8, 16]
MULTIPLEX = [True, True, True, False]  # 75% multiplex, 25% dedicated
POOL_SIZES = [2, 4, 8, 16, 32]
WRITERS = [8, 16, 32, 64, 128]
TABLES = [1, 2, 4, 8, 16, 32]
MAX_OUTSTANDING = [50, 100, 200, 500, 1000]
ROWS_PER_BATCH = [500, 1000, 2000]
ROW_SIZES = [512, 1024, 2048]


def generate_random_config():
    """Generates a random valid configuration dictionary."""
    is_multiplex = random.choice(MULTIPLEX)
    pool_size = random.choice(POOL_SIZES) if is_multiplex else 1
    channels = random.choice(CHANNELS)
    writers = random.choice(WRITERS)
    tables = random.choice(TABLES)
    max_outstanding = random.choice(MAX_OUTSTANDING)
    rows_per_batch = random.choice(ROWS_PER_BATCH)
    row_size = random.choice(ROW_SIZES)

    return {
        "grpc_channels": channels,
        "multiplex": is_multiplex,
        "multiplex_pool_size": pool_size,
        "num_writers": writers,
        "num_tables": tables,
        "max_outstanding_requests": max_outstanding,
        "rows_per_batch": rows_per_batch,
        "row_size": row_size,
    }


def parse_benchmark_output(text):
    """Parses time-series CSV rows and summary block from benchmark stdout."""
    time_series = []
    summary = {}

    csv_pattern = re.compile(
        r"^(\d+),([\d\.]+),([A-Za-z]+),(\d+),(\d+),([\d\.]+),(\d+),([\d\.]+),(\d+),([\d\.]+)"
    )

    for line in text.splitlines():
        line = line.strip()
        m = csv_pattern.match(line)
        if m:
            time_series.append({
                "timestamp": int(m.group(1)),
                "elapsed_s": float(m.group(2)),
                "op": m.group(3),
                "iteration": int(m.group(4)),
                "count": int(m.group(5)),
                "batches_per_s": float(m.group(6)),
                "bytes": int(m.group(7)),
                "mb_per_s": float(m.group(8)),
                "errors": int(m.group(9)),
                "errors_per_s": float(m.group(10)),
            })
            continue

        # Summary lines
        if line.startswith("# Elapsed time:"):
            m_val = re.search(r"([\d\.]+)s", line)
            if m_val:
                summary["elapsed_s"] = float(m_val.group(1))
        elif line.startswith("# Total batches sent:"):
            m_val = re.search(r"(\d+)", line)
            if m_val:
                summary["batches_sent"] = int(m_val.group(1))
        elif line.startswith("# Total data sent:"):
            m_val = re.search(r"([\d\.]+) MB \(rate: ([\d\.]+) batches/s, ([\d\.]+) MB/s\)", line)
            if m_val:
                summary["data_sent_mb"] = float(m_val.group(1))
                summary["send_rate_batches_s"] = float(m_val.group(2))
                summary["send_mbs"] = float(m_val.group(3))
        elif line.startswith("# Total batches completed:"):
            m_val = re.search(r"(\d+)", line)
            if m_val:
                summary["batches_completed"] = int(m_val.group(1))
        elif line.startswith("# Total data completed:"):
            m_val = re.search(r"([\d\.]+) MB \(rate: ([\d\.]+) batches/s, ([\d\.]+) MB/s\)", line)
            if m_val:
                summary["data_completed_mb"] = float(m_val.group(1))
                summary["recv_rate_batches_s"] = float(m_val.group(2))
                summary["completed_mbs"] = float(m_val.group(3))
        elif line.startswith("# Total errors:"):
            m_val = re.search(r"(\d+)", line)
            if m_val:
                summary["total_errors"] = int(m_val.group(1))

    return time_series, summary


def generate_html_report(results, output_path, title="BigQuery Write Throughput Benchmark Sweep"):
    """Generates a standalone, beautiful HTML report with SVG and Canvas charts."""
    # Filter completed runs
    valid_runs = [r for r in results if "summary" in r and "completed_mbs" in r["summary"]]
    valid_runs.sort(key=lambda r: r["summary"].get("completed_mbs", 0), reverse=True)

    json_data = json.dumps(results, indent=2)

    html_content = f"""<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>{title}</title>
  <style>
    :root {{
      --bg: #0f172a;
      --card-bg: #1e293b;
      --border: #334155;
      --text: #f8fafc;
      --text-muted: #94a3b8;
      --primary: #38bdf8;
      --accent: #818cf8;
      --success: #34d399;
      --warning: #fbbf24;
      --danger: #f87171;
    }}
    @media (prefers-color-scheme: light) {{
      :root {{
        --bg: #f8fafc;
        --card-bg: #ffffff;
        --border: #e2e8f0;
        --text: #0f172a;
        --text-muted: #64748b;
        --primary: #0284c7;
        --accent: #6366f1;
        --success: #10b981;
        --warning: #f59e0b;
        --danger: #ef4444;
      }}
    }}
    * {{ box-sizing: border-box; margin: 0; padding: 0; }}
    body {{
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
      background: var(--bg);
      color: var(--text);
      line-height: 1.5;
      padding: 2rem 1rem;
    }}
    .container {{
      max-width: 1200px;
      margin: 0 auto;
    }}
    header {{
      margin-bottom: 2rem;
      border-bottom: 1px solid var(--border);
      padding-bottom: 1.5rem;
    }}
    h1 {{ font-size: 1.875rem; font-weight: 700; color: var(--text); }}
    .subtitle {{ color: var(--text-muted); font-size: 0.875rem; margin-top: 0.25rem; }}
    
    .grid-stats {{
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      gap: 1rem;
      margin-bottom: 2rem;
    }}
    .stat-card {{
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 0.75rem;
      padding: 1.25rem;
      box-shadow: 0 1px 3px rgba(0,0,0,0.1);
    }}
    .stat-label {{ font-size: 0.75rem; font-weight: 600; text-transform: uppercase; color: var(--text-muted); letter-spacing: 0.05em; }}
    .stat-val {{ font-size: 1.75rem; font-weight: 700; color: var(--primary); margin-top: 0.25rem; }}
    .stat-sub {{ font-size: 0.75rem; color: var(--text-muted); margin-top: 0.25rem; }}
    
    .card {{
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 0.75rem;
      padding: 1.5rem;
      margin-bottom: 2rem;
      box-shadow: 0 1px 3px rgba(0,0,0,0.1);
    }}
    .card-title {{ font-size: 1.125rem; font-weight: 600; margin-bottom: 1rem; color: var(--text); }}
    
    /* Table styles */
    .table-container {{ overflow-x: auto; }}
    table {{ width: 100%; border-collapse: collapse; font-size: 0.875rem; text-align: left; }}
    th {{ background: rgba(0,0,0,0.05); padding: 0.75rem 1rem; font-weight: 600; color: var(--text-muted); border-bottom: 1px solid var(--border); cursor: pointer; }}
    th:hover {{ color: var(--primary); }}
    td {{ padding: 0.75rem 1rem; border-bottom: 1px solid var(--border); }}
    tr:hover td {{ background: rgba(56, 189, 248, 0.05); }}
    .badge {{
      display: inline-block;
      padding: 0.15rem 0.5rem;
      border-radius: 9999px;
      font-size: 0.75rem;
      font-weight: 600;
    }}
    .badge-multiplex {{ background: rgba(56, 189, 248, 0.15); color: var(--primary); border: 1px solid var(--primary); }}
    .badge-dedicated {{ background: rgba(129, 140, 248, 0.15); color: var(--accent); border: 1px solid var(--accent); }}
    
    /* Bar chart SVG */
    .bar-chart-container {{
      width: 100%;
      height: auto;
      overflow-x: auto;
    }}
    
    /* Chart tooltip */
    .tooltip {{
      position: absolute;
      background: var(--card-bg);
      border: 1px solid var(--border);
      border-radius: 0.5rem;
      padding: 0.5rem 0.75rem;
      font-size: 0.75rem;
      color: var(--text);
      box-shadow: 0 4px 6px -1px rgba(0,0,0,0.2);
      pointer-events: none;
      display: none;
      z-index: 100;
    }}
  </style>
</head>
<body>
  <div class="container">
    <header>
      <h1>{title}</h1>
      <p class="subtitle">Generated on {datetime.datetime.now().strftime('%Y-%m-%d %H:%M:%S UTC')} • {len(valid_runs)} total runs analyzed</p>
    </header>

    <div class="grid-stats" id="statsGrid">
      <!-- Injected via JavaScript -->
    </div>

    <!-- Leaderboard Chart -->
    <div class="card">
      <h2 class="card-title">🏆 Throughput Leaderboard (MB/s)</h2>
      <div class="bar-chart-container" id="leaderboardChart"></div>
    </div>

    <!-- Time Series Chart -->
    <div class="card">
      <h2 class="card-title">📈 Throughput Over Time (All Runs)</h2>
      <div style="position: relative; width: 100%; height: 360px;">
        <canvas id="timeSeriesCanvas" width="1100" height="340" style="width: 100%; height: 100%;"></canvas>
      </div>
      <div id="seriesLegend" style="display: flex; flex-wrap: wrap; gap: 0.75rem; margin-top: 1rem; font-size: 0.75rem;"></div>
    </div>

    <!-- Results Table -->
    <div class="card">
      <h2 class="card-title">📊 Detailed Run Results</h2>
      <div class="table-container">
        <table id="resultsTable">
          <thead>
            <tr>
              <th onclick="sortTable(0)">Run</th>
              <th onclick="sortTable(1)">Throughput (MB/s)</th>
              <th onclick="sortTable(2)">Rate (batches/s)</th>
              <th onclick="sortTable(3)">Channels</th>
              <th onclick="sortTable(4)">Mode</th>
              <th onclick="sortTable(5)">Pool</th>
              <th onclick="sortTable(6)">Writers</th>
              <th onclick="sortTable(7)">Tables</th>
              <th onclick="sortTable(8)">Batch (KB)</th>
              <th onclick="sortTable(9)">Max Outstanding</th>
              <th onclick="sortTable(10)">Errors</th>
            </tr>
          </thead>
          <tbody id="tableBody">
            <!-- Injected via JavaScript -->
          </tbody>
        </table>
      </div>
    </div>
  </div>

  <div id="chartTooltip" class="tooltip"></div>

  <script>
    const RUNS_DATA = {json_data};

    function init() {{
      renderStats();
      renderLeaderboard();
      renderTimeSeries();
      renderTable();
    }}

    function renderStats() {{
      const valid = RUNS_DATA.filter(r => r.summary && r.summary.completed_mbs !== undefined);
      if (valid.length === 0) return;

      const topRun = [...valid].sort((a,b) => b.summary.completed_mbs - a.summary.completed_mbs)[0];
      const avgMbs = valid.reduce((acc, r) => acc + r.summary.completed_mbs, 0) / valid.length;
      const totalMb = valid.reduce((acc, r) => acc + (r.summary.data_completed_mb || 0), 0);
      const totalErrors = valid.reduce((acc, r) => acc + (r.summary.total_errors || 0), 0);

      const grid = document.getElementById('statsGrid');
      grid.innerHTML = `
        <div class="stat-card">
          <div class="stat-label">Peak Throughput</div>
          <div class="stat-val">${{topRun.summary.completed_mbs.toFixed(2)}} <span style="font-size: 1rem;">MB/s</span></div>
          <div class="stat-sub">Run #${{topRun.run_id}} (${{topRun.config.grpc_channels}} chan, pool ${{topRun.config.multiplex_pool_size}})</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Average Throughput</div>
          <div class="stat-val">${{avgMbs.toFixed(2)}} <span style="font-size: 1rem;">MB/s</span></div>
          <div class="stat-sub">Across ${{valid.length}} runs</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Total Data Ingested</div>
          <div class="stat-val">${{(totalMb / 1024).toFixed(2)}} <span style="font-size: 1rem;">GB</span></div>
          <div class="stat-sub">${{totalMb.toLocaleString()}} MB total</div>
        </div>
        <div class="stat-card">
          <div class="stat-label">Total Errors</div>
          <div class="stat-val" style="color: ${{totalErrors === 0 ? 'var(--success)' : 'var(--danger)'}}">${{totalErrors}}</div>
          <div class="stat-sub">${{totalErrors === 0 ? '100% successful batches' : 'Errors recorded'}}</div>
        </div>
      `;
    }}

    function renderLeaderboard() {{
      const valid = RUNS_DATA.filter(r => r.summary && r.summary.completed_mbs !== undefined);
      if (valid.length === 0) return;

      const sorted = [...valid].sort((a,b) => b.summary.completed_mbs - a.summary.completed_mbs);
      const maxMbs = sorted[0].summary.completed_mbs;

      const container = document.getElementById('leaderboardChart');
      const barHeight = 36;
      const svgHeight = sorted.length * (barHeight + 12) + 20;

      let barsHtml = '';
      sorted.forEach((run, idx) => {{
        const y = idx * (barHeight + 12) + 10;
        const widthPercent = (run.summary.completed_mbs / (maxMbs * 1.1)) * 100;
        const isMux = run.config.multiplex;
        const color = isMux ? 'var(--primary)' : 'var(--accent)';
        const label = `Run #${{run.run_id}}: ${{run.config.num_writers}} writers, ${{run.config.grpc_channels}} chan, ${{isMux ? 'pool ' + run.config.multiplex_pool_size : 'dedicated'}}`;

        barsHtml += `
          <g class="bar-group" onmouseover="showTooltip(event, '${{run.run_id}}')" onmouseout="hideTooltip()">
            <text x="10" y="${{y + 14}}" fill="var(--text-muted)" font-size="11" font-weight="600">${{label}}</text>
            <rect x="10" y="${{y + 20}}" width="${{Math.max(widthPercent * 7, 20)}}" height="18" rx="4" fill="${{color}}" opacity="0.85"></rect>
            <text x="${{Math.max(widthPercent * 7, 20) + 18}}" y="${{y + 34}}" fill="var(--text)" font-size="12" font-weight="700">${{run.summary.completed_mbs.toFixed(2)}} MB/s</text>
          </g>
        `;
      }});

      container.innerHTML = `
        <svg width="100%" height="${{svgHeight}}" style="overflow: visible;">
          ${{barsHtml}}
        </svg>
      `;
    }}

    function renderTimeSeries() {{
      const canvas = document.getElementById('timeSeriesCanvas');
      const ctx = canvas.getContext('2d');
      const dpr = window.devicePixelRatio || 1;
      const rect = canvas.getBoundingClientRect();
      canvas.width = rect.width * dpr;
      canvas.height = rect.height * dpr;
      ctx.scale(dpr, dpr);

      const w = rect.width;
      const h = rect.height;
      const padding = {{ top: 20, right: 30, bottom: 40, left: 55 }};

      // Collect all series
      const colors = ['#38bdf8', '#818cf8', '#34d399', '#f472b6', '#fbbf24', '#a78bfa', '#f87171', '#4ade80'];
      const validRuns = RUNS_DATA.filter(r => r.time_series && r.time_series.length > 0);

      let maxTime = 60;
      let maxRate = 50;

      validRuns.forEach(r => {{
        r.time_series.forEach(pt => {{
          if (pt.elapsed_s > maxTime) maxTime = pt.elapsed_s;
          if (pt.mb_per_s > maxRate) maxRate = pt.mb_per_s;
        }});
      }});
      maxRate = Math.ceil(maxRate * 1.15);

      // Draw grid
      ctx.strokeStyle = 'rgba(255, 255, 255, 0.08)';
      ctx.lineWidth = 1;
      ctx.fillStyle = '#94a3b8';
      ctx.font = '11px sans-serif';

      const numYSteps = 5;
      for (let i = 0; i <= numYSteps; i++) {{
        const yVal = (maxRate / numYSteps) * i;
        const yPos = h - padding.bottom - (yVal / maxRate) * (h - padding.top - padding.bottom);
        ctx.beginPath();
        ctx.moveTo(padding.left, yPos);
        ctx.lineTo(w - padding.right, yPos);
        ctx.stroke();
        ctx.fillText(`${{yVal.toFixed(0)}} MB/s`, 5, yPos + 4);
      }}

      // X-axis steps
      const numXSteps = 6;
      for (let i = 0; i <= numXSteps; i++) {{
        const xVal = (maxTime / numXSteps) * i;
        const xPos = padding.left + (xVal / maxTime) * (w - padding.left - padding.right);
        ctx.beginPath();
        ctx.moveTo(xPos, h - padding.bottom);
        ctx.lineTo(xPos, padding.top);
        ctx.stroke();
        ctx.fillText(`${{xVal.toFixed(0)}}s`, xPos - 10, h - padding.bottom + 20);
      }}

      // Plot lines
      const legend = document.getElementById('seriesLegend');
      legend.innerHTML = '';

      validRuns.forEach((r, idx) => {{
        const color = colors[idx % colors.length];
        const series = r.time_series.filter(pt => pt.op === 'Recv');
        if (series.length === 0) return;

        ctx.strokeStyle = color;
        ctx.lineWidth = 2.5;
        ctx.beginPath();

        series.forEach((pt, ptIdx) => {{
          const x = padding.left + (pt.elapsed_s / maxTime) * (w - padding.left - padding.right);
          const y = h - padding.bottom - (pt.mb_per_s / maxRate) * (h - padding.top - padding.bottom);
          if (ptIdx === 0) ctx.moveTo(x, y);
          else ctx.lineTo(x, y);
        }});
        ctx.stroke();

        legend.innerHTML += `
          <div style="display: flex; align-items: center; gap: 0.35rem;">
            <span style="display: inline-block; width: 12px; height: 12px; border-radius: 3px; background: ${{color}};"></span>
            <span>Run #${{r.run_id}} (${{r.summary.completed_mbs.toFixed(1)}} MB/s)</span>
          </div>
        `;
      }});
    }}

    function renderTable() {{
      const tbody = document.getElementById('tableBody');
      tbody.innerHTML = '';

      RUNS_DATA.forEach(r => {{
        const isMux = r.config.multiplex;
        const batchKb = ((r.config.rows_per_batch * r.config.row_size) / 1024).toFixed(0);
        const mbs = r.summary && r.summary.completed_mbs !== undefined ? r.summary.completed_mbs.toFixed(2) : 'N/A';
        const rate = r.summary && r.summary.recv_rate_batches_s !== undefined ? r.summary.recv_rate_batches_s.toFixed(1) : 'N/A';
        const errors = r.summary && r.summary.total_errors !== undefined ? r.summary.total_errors : '0';

        tbody.innerHTML += `
          <tr>
            <td style="font-weight: 600;">#${{r.run_id}}</td>
            <td style="font-weight: 700; color: var(--primary);">${{mbs}}</td>
            <td>${{rate}}</td>
            <td>${{r.config.grpc_channels}}</td>
            <td><span class="badge ${{isMux ? 'badge-multiplex' : 'badge-dedicated'}}">${{isMux ? 'Multiplex' : 'Dedicated'}}</span></td>
            <td>${{isMux ? r.config.multiplex_pool_size : '-'}}</td>
            <td>${{r.config.num_writers}}</td>
            <td>${{r.config.num_tables}}</td>
            <td>${{batchKb}} KB</td>
            <td>${{r.config.max_outstanding_requests || 'unbounded'}}</td>
            <td style="color: ${{errors === 0 ? 'var(--success)' : 'var(--danger)'}}; font-weight: 600;">${{errors}}</td>
          </tr>
        `;
      }});
    }}

    let sortAsc = true;
    function sortTable(colIndex) {{
      // Sort in-place and re-render
      RUNS_DATA.sort((a, b) => {{
        let vA, vB;
        if (colIndex === 0) {{ vA = a.run_id; vB = b.run_id; }}
        else if (colIndex === 1) {{ vA = a.summary?.completed_mbs || 0; vB = b.summary?.completed_mbs || 0; }}
        else if (colIndex === 2) {{ vA = a.summary?.recv_rate_batches_s || 0; vB = b.summary?.recv_rate_batches_s || 0; }}
        else if (colIndex === 3) {{ vA = a.config.grpc_channels; vB = b.config.grpc_channels; }}
        else if (colIndex === 6) {{ vA = a.config.num_writers; vB = b.config.num_writers; }}
        else if (colIndex === 7) {{ vA = a.config.num_tables; vB = b.config.num_tables; }}
        else if (colIndex === 10) {{ vA = a.summary?.total_errors || 0; vB = b.summary?.total_errors || 0; }}
        else return 0;
        return sortAsc ? vA - vB : vB - vA;
      }});
      sortAsc = !sortAsc;
      renderTable();
    }}

    window.addEventListener('load', init);
    window.addEventListener('resize', renderTimeSeries);
  </script>
</body>
</html>
"""

    with open(output_path, "w", encoding="utf-8") as f:
        f.write(html_content)


def run_sweep(args):
    """Executes N benchmark runs with random settings, recording logs and updating dashboard."""
    project = args.project or os.environ.get("GOOGLE_CLOUD_PROJECT")
    if not project:
        print("Error: --project or GOOGLE_CLOUD_PROJECT environment variable must be set.")
        sys.exit(1)

    bin_path = Path(args.bin).resolve()
    if not bin_path.is_file():
        # Try local target/release
        alt_path = Path("target/release/bigquery-write-throughput").resolve()
        if alt_path.is_file():
            bin_path = alt_path
        else:
            print(f"Error: Benchmark binary not found at {bin_path}. Run 'cargo build --release -p bigquery-write-throughput' first.")
            sys.exit(1)

    timestamp = int(time.time())
    output_dir = Path(args.output_dir or f"sweep_results_{timestamp}").resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    logs_dir = output_dir / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)

    summary_csv = output_dir / "summary.csv"
    summary_json = output_dir / "summary.json"
    report_html = output_dir / "report.html"

    print("=" * 80)
    print(f"🚀 Starting BigQuery Write Throughput Automated Sweep")
    print(f"   Runs: {args.num_runs}")
    print(f"   Duration per run: {args.duration}")
    print(f"   Report interval: {args.report_interval}")
    print(f"   Binary: {bin_path}")
    print(f"   Output Directory: {output_dir}")
    print("=" * 80)

    results = []

    try:
        for i in range(1, args.num_runs + 1):
            config = generate_random_config()
            run_id = f"{i:03d}"
            log_file = logs_dir / f"run_{run_id}.txt"

            cmd = [
                str(bin_path),
                "--project", project,
                "--duration", args.duration,
                "--report-interval", args.report_interval,
                "--num-writers", str(config["num_writers"]),
                "--num-tables", str(config["num_tables"]),
                "--grpc-channels", str(config["grpc_channels"]),
                "--row-size", str(config["row_size"]),
                "--rows-per-batch", str(config["rows_per_batch"]),
            ]
            if config["multiplex"]:
                cmd.extend(["--multiplex", "--multiplex-pool-size", str(config["multiplex_pool_size"])])
            if config["max_outstanding_requests"]:
                cmd.extend(["--max-outstanding-requests", str(config["max_outstanding_requests"])])

            print(f"\n[{i}/{args.num_runs}] Running test #{run_id}:")
            print(f"   Channels: {config['grpc_channels']} | Writers: {config['num_writers']} | Tables: {config['num_tables']} | Multiplex: {config['multiplex']} (pool: {config['multiplex_pool_size']}) | Batch: {config['rows_per_batch']}x{config['row_size']}B | MaxOut: {config['max_outstanding_requests']}")

            start_t = time.time()
            proc = subprocess.Popen(
                cmd,
                stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT,
                text=True,
                bufsize=1,
            )

            captured_output = []
            while True:
                line = proc.stdout.readline()
                if not line and proc.poll() is not None:
                    break
                if line:
                    captured_output.append(line)
                    # Print progress lines
                    if line.startswith("#") or "Recv," in line:
                        print("   " + line.strip())

            proc.wait()
            elapsed_run = time.time() - start_t
            full_text = "".join(captured_output)

            with open(log_file, "w", encoding="utf-8") as f:
                f.write(full_text)

            time_series, summary = parse_benchmark_output(full_text)
            completed_mbs = summary.get("completed_mbs", 0.0)
            total_errors = summary.get("total_errors", 0)

            print(f"   Finished in {elapsed_run:.1f}s -> Throughput: {completed_mbs:.2f} MB/s | Errors: {total_errors}")

            run_record = {
                "run_id": i,
                "timestamp": int(time.time()),
                "config": config,
                "summary": summary,
                "time_series": time_series,
            }
            results.append(run_record)

            # Persist summary JSON
            with open(summary_json, "w", encoding="utf-8") as f:
                json.dump(results, f, indent=2)

            # Persist summary CSV
            with open(summary_csv, "w", newline="", encoding="utf-8") as f:
                writer = csv.writer(f)
                writer.writerow([
                    "run_id", "completed_mbs", "recv_rate_batches_s", "channels",
                    "multiplex", "pool_size", "writers", "tables", "row_size",
                    "rows_per_batch", "max_outstanding", "errors", "elapsed_s"
                ])
                for r in results:
                    s = r.get("summary", {})
                    c = r["config"]
                    writer.writerow([
                        r["run_id"],
                        s.get("completed_mbs", 0.0),
                        s.get("recv_rate_batches_s", 0.0),
                        c["grpc_channels"],
                        c["multiplex"],
                        c["multiplex_pool_size"],
                        c["num_writers"],
                        c["num_tables"],
                        c["row_size"],
                        c["rows_per_batch"],
                        c.get("max_outstanding_requests", ""),
                        s.get("total_errors", 0),
                        s.get("elapsed_s", 0.0)
                    ])

            # Update HTML report after every run
            generate_html_report(results, report_html)
            print(f"   Updated interactive dashboard: file://{report_html}")

    except KeyboardInterrupt:
        print("\n\n⚠️ Sweep interrupted by user. Preserving completed runs and generating final report...")
    finally:
        if results:
            generate_html_report(results, report_html)
            print("\n" + "=" * 80)
            print(f"✅ Sweep complete! Summary and interactive graphs saved to:")
            print(f"   HTML Dashboard: file://{report_html}")
            print(f"   Summary CSV:    file://{summary_csv}")
            print("=" * 80)


def plot_single_log(log_path):
    """Parses an existing benchmark log (e.g. 8-hour or 48-hour run) and generates an HTML graph."""
    path = Path(log_path).resolve()
    if not path.is_file():
        print(f"Error: file not found: {path}")
        sys.exit(1)

    print(f"Parsing log file: {path}...")
    with open(path, "r", encoding="utf-8", errors="replace") as f:
        text = f.read()

    time_series, summary = parse_benchmark_output(text)
    if not time_series and not summary:
        print("Warning: Could not parse any time-series or summary data from log file.")

    run_record = {
        "run_id": 1,
        "timestamp": int(time.time()),
        "config": {
            "grpc_channels": "auto",
            "multiplex": True,
            "multiplex_pool_size": "auto",
            "num_writers": "auto",
            "num_tables": "auto",
            "row_size": 1024,
            "rows_per_batch": 1000,
        },
        "summary": summary,
        "time_series": time_series,
    }

    out_html = path.parent / f"{path.stem}_graph.html"
    generate_html_report([run_record], out_html, title=f"BigQuery Write Throughput: {path.name}")
    print(f"✅ Generated graph report: file://{out_html}")


def main():
    parser = argparse.ArgumentParser(description="BigQuery Write Throughput Automated Sweep and Graph Generator")
    parser.add_argument("-n", "--num-runs", type=int, default=10, help="Number of random runs (default: 10)")
    parser.add_argument("--duration", type=str, default="5m", help="Duration for each run (e.g. 5m, 300s, default: 5m)")
    parser.add_argument("--report-interval", type=str, default="10s", help="Report interval (default: 10s)")
    parser.add_argument("--project", type=str, help="Google Cloud project ID (defaults to GOOGLE_CLOUD_PROJECT env var)")
    parser.add_argument("--bin", type=str, default="target/release/bigquery-write-throughput", help="Path to benchmark binary")
    parser.add_argument("--output-dir", type=str, help="Output directory for results")
    parser.add_argument("--plot-log", type=str, help="Plot an existing log file directly without running new benchmarks")

    args = parser.parse_args()

    if args.plot_log:
        plot_single_log(args.plot_log)
    else:
        run_sweep(args)


if __name__ == "__main__":
    main()
