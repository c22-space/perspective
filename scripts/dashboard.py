#!/usr/bin/env python3
"""Perspective Dashboard - standalone Python server.

Serves the dashboard HTML and /api/status endpoint
using the in-process Perspective engine.

Usage:
    python3 dashboard.py [--port 8080] [--data-dir ~/.hermes/perspective/data]
"""

import json
import os
import sys
import time
import argparse
from http.server import HTTPServer, BaseHTTPRequestHandler
from pathlib import Path
from datetime import datetime

# Add hermes-agent to path for imports
sys.path.insert(0, os.path.expanduser("~/.hermes/hermes-agent"))

DEFAULT_PORT = 8080
DEFAULT_DATA_DIR = os.path.expanduser("~/.hermes/perspective/data")
DEFAULT_TENANT = "hermes"
_start_time = time.time()


def get_engine_stats(engine, tenant_id):
    """Collect stats from the perspective engine."""
    try:
        health = engine.health()
    except Exception:
        health = {"status": "degraded"}

    # Count memories by trying recall with broad queries
    stats = {
        "health": "healthy" if health else "degraded",
        "uptime_secs": int(time.time() - _start_time),
        "total_memories": 0,
        "tenant_count": 1,
        "memory_types": {"episodic": 0, "semantic": 0, "procedural": 0},
        "gc_candidates": 0,
        "decay_config": {
            "episodic_lambda": 0.1,
            "semantic_lambda": 0.01,
            "procedural_lambda": 0.0,
            "learning_rate": 0.2,
            "retrieval_threshold": 0.01,
            "gc_threshold": 0.05,
        },
        "recent_activity": [],
    }

    # Try to get counts via recall
    for mtype in ["episodic", "semantic", "procedural"]:
        try:
            results = engine.recall(tenant_id, mtype, budget=1000)
            count = len(results) if results else 0
            stats["memory_types"][mtype] = count
            stats["total_memories"] += count

            # Add recent activity
            for r in results[:5]:
                if hasattr(r, "content"):
                    stats["recent_activity"].append({
                        "tenant_id": tenant_id,
                        "memory_type": mtype,
                        "content": r.content[:120] if r.content else "",
                        "timestamp": datetime.now().isoformat(),
                    })
        except Exception:
            pass

    return stats


DASHBOARD_HTML = r"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Perspective Dashboard</title>
<style>
  * { margin: 0; padding: 0; box-sizing: border-box; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
         background: #0f1117; color: #e0e0e0; padding: 24px; }
  h1 { font-size: 1.6rem; margin-bottom: 8px; color: #fff; }
  .subtitle { color: #888; margin-bottom: 24px; font-size: 0.9rem; }
  .grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
           gap: 16px; margin-bottom: 24px; }
  .card { background: #1a1d27; border: 1px solid #2a2d3a; border-radius: 10px;
           padding: 20px; }
  .card h2 { font-size: 1rem; color: #8b8fa3; margin-bottom: 12px;
              text-transform: uppercase; letter-spacing: 0.5px; }
  .metric { font-size: 2.2rem; font-weight: 700; color: #fff; }
  .metric-label { font-size: 0.8rem; color: #6b6f83; margin-top: 4px; }
  .status-dot { display: inline-block; width: 10px; height: 10px;
                 border-radius: 50%; margin-right: 8px; }
  .status-healthy { background: #22c55e; box-shadow: 0 0 6px #22c55e66; }
  .status-degraded { background: #f59e0b; box-shadow: 0 0 6px #f59e0b66; }
  .status-down { background: #ef4444; box-shadow: 0 0 6px #ef444466; }
  table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
  th { text-align: left; color: #888; padding: 8px 12px;
       border-bottom: 1px solid #2a2d3a; }
  td { padding: 8px 12px; border-bottom: 1px solid #1f2233; }
  .bar-bg { background: #2a2d3a; border-radius: 4px; height: 8px; }
  .bar { border-radius: 4px; height: 8px; transition: width 0.5s; }
  .bar-episodic { background: #3b82f6; }
  .bar-semantic { background: #8b5cf6; }
  .bar-procedural { background: #f59e0b; }
  #loading { text-align: center; padding: 40px; color: #888; }
  .footer { margin-top: 32px; text-align: center; color: #555; font-size: 0.75rem; }
  .logo { display: inline-flex; align-items: center; gap: 8px; margin-bottom: 4px; }
  .logo svg { width: 24px; height: 24px; }
</style>
</head>
<body>
  <div class="logo">
    <svg viewBox="0 0 24 24" fill="none" stroke="#3b82f6" stroke-width="2">
      <circle cx="12" cy="12" r="3"/>
      <circle cx="4" cy="6" r="2"/>
      <circle cx="20" cy="6" r="2"/>
      <circle cx="4" cy="18" r="2"/>
      <circle cx="20" cy="18" r="2"/>
      <line x1="6" y1="7" x2="10" y2="10"/>
      <line x1="18" y1="7" x2="14" y2="10"/>
      <line x1="6" y1="17" x2="10" y2="14"/>
      <line x1="18" y1="17" x2="14" y2="14"/>
    </svg>
    <h1>Perspective</h1>
  </div>
  <p class="subtitle">Memory Engine Dashboard</p>

  <div id="loading">Connecting to engine...</div>

  <div class="grid" id="stats" style="display:none">
    <div class="card">
      <h2>Health</h2>
      <div>
        <span class="status-dot status-healthy" id="health-dot"></span>
        <span class="metric" id="health-label">Healthy</span>
      </div>
      <div class="metric-label" id="uptime"></div>
    </div>
    <div class="card">
      <h2>Total Memories</h2>
      <div class="metric" id="total-memories">0</div>
      <div class="metric-label">across all tenants</div>
    </div>
    <div class="card">
      <h2>Tenants</h2>
      <div class="metric" id="tenant-count">0</div>
      <div class="metric-label">active tenants</div>
    </div>
    <div class="card">
      <h2>GC Candidates</h2>
      <div class="metric" id="gc-candidates">0</div>
      <div class="metric-label">below decay threshold</div>
    </div>
  </div>

  <div class="grid" style="grid-template-columns: 1fr 1fr">
    <div class="card">
      <h2>Memory Types</h2>
      <table>
        <thead><tr><th>Type</th><th>Count</th><th style="width:40%">Distribution</th></tr></thead>
        <tbody>
          <tr><td>Episodic</td><td id="ep-count">0</td>
              <td><div class="bar-bg"><div class="bar bar-episodic" id="ep-bar" style="width:0%"></div></div></td></tr>
          <tr><td>Semantic</td><td id="sem-count">0</td>
              <td><div class="bar-bg"><div class="bar bar-semantic" id="sem-bar" style="width:0%"></div></div></td></tr>
          <tr><td>Procedural</td><td id="proc-count">0</td>
              <td><div class="bar-bg"><div class="bar bar-procedural" id="proc-bar" style="width:0%"></div></div></td></tr>
        </tbody>
      </table>
    </div>
    <div class="card">
      <h2>Decay Config</h2>
      <table>
        <thead><tr><th>Parameter</th><th>Value</th></tr></thead>
        <tbody id="decay-table"></tbody>
      </table>
    </div>
  </div>

  <div class="card">
    <h2>Recent Activity</h2>
    <table>
      <thead><tr><th>Time</th><th>Tenant</th><th>Type</th><th>Content</th></tr></thead>
      <tbody id="activity-table"><tr><td colspan="4" style="color:#666">Loading...</td></tr></tbody>
    </table>
  </div>

  <p class="footer">Perspective Memory Engine &mdash; auto-refreshes every 5s</p>

<script>
(function() {
  function fmt(n) { return n.toLocaleString(); }

  function render(data) {
    document.getElementById('loading').style.display = 'none';
    document.getElementById('stats').style.display = '';

    var health = data.health || 'unknown';
    var dot = document.getElementById('health-dot');
    dot.className = 'status-dot ' + (health === 'healthy' ? 'status-healthy' :
                     health === 'degraded' ? 'status-degraded' : 'status-down');
    document.getElementById('health-label').textContent = health.charAt(0).toUpperCase() + health.slice(1);
    if (data.uptime_secs != null) {
      var h = Math.floor(data.uptime_secs / 3600);
      var m = Math.floor((data.uptime_secs % 3600) / 60);
      document.getElementById('uptime').textContent = 'Uptime: ' + h + 'h ' + m + 'm';
    }

    document.getElementById('total-memories').textContent = fmt(data.total_memories || 0);
    document.getElementById('tenant-count').textContent = fmt(data.tenant_count || 0);
    document.getElementById('gc-candidates').textContent = fmt(data.gc_candidates || 0);

    var types = data.memory_types || {};
    var ep = types.episodic || 0, sem = types.semantic || 0, proc = types.procedural || 0;
    var total = ep + sem + proc || 1;
    document.getElementById('ep-count').textContent = fmt(ep);
    document.getElementById('sem-count').textContent = fmt(sem);
    document.getElementById('proc-count').textContent = fmt(proc);
    document.getElementById('ep-bar').style.width = (ep / total * 100) + '%';
    document.getElementById('sem-bar').style.width = (sem / total * 100) + '%';
    document.getElementById('proc-bar').style.width = (proc / total * 100) + '%';

    var decay = data.decay_config || {};
    var dtbody = document.getElementById('decay-table');
    dtbody.innerHTML = '';
    var keys = ['episodic_lambda','semantic_lambda','procedural_lambda',
                'learning_rate','retrieval_threshold','gc_threshold'];
    keys.forEach(function(k) {
      var tr = document.createElement('tr');
      tr.innerHTML = '<td>' + k.replace(/_/g, ' ') + '</td><td>' + (decay[k] != null ? decay[k] : '—') + '</td>';
      dtbody.appendChild(tr);
    });

    var activity = data.recent_activity || [];
    var abody = document.getElementById('activity-table');
    if (activity.length === 0) {
      abody.innerHTML = '<tr><td colspan="4" style="color:#666">No activity yet</td></tr>';
    } else {
      abody.innerHTML = '';
      activity.forEach(function(a) {
        var tr = document.createElement('tr');
        var ts = a.timestamp ? new Date(a.timestamp).toLocaleTimeString() : '—';
        tr.innerHTML = '<td>' + ts + '</td><td>' + (a.tenant_id || '—') + '</td>' +
                       '<td>' + (a.memory_type || '—') + '</td><td>' + (a.content || '').substring(0, 80) + '</td>';
        abody.appendChild(tr);
      });
    }
  }

  function poll() {
    fetch('/api/status')
      .then(function(r) { return r.json(); })
      .then(render)
      .catch(function() {
        document.getElementById('loading').textContent = 'Engine unavailable';
      });
  }

  poll();
  setInterval(poll, 5000);
})();
</script>
</body>
</html>"""


class DashboardHandler(BaseHTTPRequestHandler):
    """HTTP handler for the dashboard."""

    engine = None
    tenant_id = DEFAULT_TENANT

    def do_GET(self):
        if self.path == "/api/status":
            self._handle_status()
        elif self.path == "/" or self.path == "/index.html":
            self._handle_dashboard()
        else:
            self.send_error(404)

    def _handle_status(self):
        stats = get_engine_stats(self.engine, self.tenant_id)
        body = json.dumps(stats).encode()
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Access-Control-Allow-Origin", "*")
        self.end_headers()
        self.wfile.write(body)

    def _handle_dashboard(self):
        # Get initial stats for the page
        stats = get_engine_stats(self.engine, self.tenant_id)
        stats_json = json.dumps(stats)

        # Inject stats into HTML
        html = DASHBOARD_HTML.replace(
            "const DATA = {};",
            f"const DATA = {stats_json};"
        )
        # The HTML above doesn't have a DATA variable, so we inject via /api/status on load
        body = html.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/html")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, format, *args):
        # Suppress request logging
        pass


def main():
    parser = argparse.ArgumentParser(description="Perspective Dashboard")
    parser.add_argument("--port", type=int, default=DEFAULT_PORT, help="Port to listen on")
    parser.add_argument("--data-dir", default=DEFAULT_DATA_DIR, help="Perspective data directory")
    parser.add_argument("--tenant", default=DEFAULT_TENANT, help="Tenant ID")
    parser.add_argument("--host", default="0.0.0.0", help="Host to bind to")
    args = parser.parse_args()

    # Import and create engine
    try:
        from perspective_python import PerspectiveEngine
    except ImportError:
        print("ERROR: perspective-python not installed.")
        print("Run: cd /home/charlie/perspective/crates/perspective-python && maturin develop")
        sys.exit(1)

    print(f"Starting Perspective Dashboard...")
    print(f"  Data dir: {args.data_dir}")
    print(f"  Tenant:   {args.tenant}")

    os.makedirs(args.data_dir, exist_ok=True)

    try:
        engine = PerspectiveEngine(args.data_dir)
        print(f"  Engine:   OK")
    except Exception as e:
        print(f"  Engine:   FAILED ({e})")
        sys.exit(1)

    DashboardHandler.engine = engine
    DashboardHandler.tenant_id = args.tenant

    server = HTTPServer((args.host, args.port), DashboardHandler)
    print(f"\nDashboard: http://localhost:{args.port}")
    print(f"API:       http://localhost:{args.port}/api/status")
    print(f"\nPress Ctrl+C to stop.\n")

    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nShutting down...")
        server.shutdown()


if __name__ == "__main__":
    main()
