/// Simple HTML dashboard for the Perspective memory engine.
///
/// This module embeds a complete single-page dashboard as a string literal.
/// It can be served via any HTTP server (axum, actix, warp, etc.).
///
/// The dashboard displays:
/// - Engine health status
/// - Memory statistics (total count, counts by type)
/// - Recent activity
/// - Decay metrics (Ebbinghaus curve, stability averages)

/// Returns the full HTML for the dashboard page.
/// `stats_json` should be a serde_json::Value with the engine status.
pub fn dashboard_html(stats_json: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Perspective Dashboard</title>
<style>
  * {{ margin: 0; padding: 0; box-sizing: border-box; }}
  body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
         background: #0f1117; color: #e0e0e0; padding: 24px; }}
  h1 {{ font-size: 1.6rem; margin-bottom: 8px; color: #fff; }}
  .subtitle {{ color: #888; margin-bottom: 24px; font-size: 0.9rem; }}
  .grid {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(280px, 1fr));
           gap: 16px; margin-bottom: 24px; }}
  .card {{ background: #1a1d27; border: 1px solid #2a2d3a; border-radius: 10px;
           padding: 20px; }}
  .card h2 {{ font-size: 1rem; color: #8b8fa3; margin-bottom: 12px;
              text-transform: uppercase; letter-spacing: 0.5px; }}
  .metric {{ font-size: 2.2rem; font-weight: 700; color: #fff; }}
  .metric-label {{ font-size: 0.8rem; color: #6b6f83; margin-top: 4px; }}
  .status-dot {{ display: inline-block; width: 10px; height: 10px;
                 border-radius: 50%; margin-right: 8px; }}
  .status-healthy {{ background: #22c55e; box-shadow: 0 0 6px #22c55e66; }}
  .status-degraded {{ background: #f59e0b; box-shadow: 0 0 6px #f59e0b66; }}
  .status-down {{ background: #ef4444; box-shadow: 0 0 6px #ef444466; }}
  table {{ width: 100%; border-collapse: collapse; font-size: 0.85rem; }}
  th {{ text-align: left; color: #888; padding: 8px 12px;
        border-bottom: 1px solid #2a2d3a; }}
  td {{ padding: 8px 12px; border-bottom: 1px solid #1f2233; }}
  .bar-bg {{ background: #2a2d3a; border-radius: 4px; height: 8px; }}
  .bar {{ border-radius: 4px; height: 8px; transition: width 0.5s; }}
  .bar-episodic {{ background: #3b82f6; }}
  .bar-semantic {{ background: #8b5cf6; }}
  .bar-procedural {{ background: #f59e0b; }}
  #loading {{ text-align: center; padding: 40px; color: #888; }}
  .footer {{ margin-top: 32px; text-align: center; color: #555; font-size: 0.75rem; }}
</style>
</head>
<body>
  <h1>Perspective Engine Dashboard</h1>
  <p class="subtitle">Real-time memory engine monitoring</p>

  <div id="loading">Loading engine status...</div>

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
      <h2>Decay</h2>
      <div class="metric" id="gc-candidates">0</div>
      <div class="metric-label">GC candidates (below threshold)</div>
    </div>
  </div>

  <div class="grid" style="grid-template-columns: 1fr 1fr">
    <div class="card">
      <h2>Memory Types</h2>
      <table>
        <thead><tr><th>Type</th><th>Count</th><th style="width:40%">Distribution</th></tr></thead>
        <tbody id="type-table">
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
      <tbody id="activity-table"><tr><td colspan="4" style="color:#666">No activity yet</td></tr></tbody>
    </table>
  </div>

  <p class="footer">Perspective Memory Engine &mdash; dashboard auto-refreshes every 5s</p>

<script>
(function() {{
  const DATA = {stats_json};

  function fmt(n) {{ return n.toLocaleString(); }}

  function render(data) {{
    document.getElementById('loading').style.display = 'none';
    document.getElementById('stats').style.display = '';

    // Health
    var health = data.health || 'unknown';
    var dot = document.getElementById('health-dot');
    dot.className = 'status-dot ' + (health === 'healthy' ? 'status-healthy' :
                     health === 'degraded' ? 'status-degraded' : 'status-down');
    document.getElementById('health-label').textContent = health.charAt(0).toUpperCase() + health.slice(1);
    if (data.uptime_secs != null) {{
      var h = Math.floor(data.uptime_secs / 3600);
      var m = Math.floor((data.uptime_secs % 3600) / 60);
      document.getElementById('uptime').textContent = 'Uptime: ' + h + 'h ' + m + 'm';
    }}

    // Totals
    document.getElementById('total-memories').textContent = fmt(data.total_memories || 0);
    document.getElementById('tenant-count').textContent = fmt(data.tenant_count || 0);
    document.getElementById('gc-candidates').textContent = fmt(data.gc_candidates || 0);

    // Types
    var types = data.memory_types || {{}};
    var ep = types.episodic || 0, sem = types.semantic || 0, proc = types.procedural || 0;
    var total = ep + sem + proc || 1;
    document.getElementById('ep-count').textContent = fmt(ep);
    document.getElementById('sem-count').textContent = fmt(sem);
    document.getElementById('proc-count').textContent = fmt(proc);
    document.getElementById('ep-bar').style.width = (ep / total * 100) + '%';
    document.getElementById('sem-bar').style.width = (sem / total * 100) + '%';
    document.getElementById('proc-bar').style.width = (proc / total * 100) + '%';

    // Decay config
    var decay = data.decay_config || {{}};
    var dtbody = document.getElementById('decay-table');
    dtbody.innerHTML = '';
    var keys = ['episodic_lambda','semantic_lambda','procedural_lambda',
                'learning_rate','retrieval_threshold','gc_threshold'];
    keys.forEach(function(k) {{
      var tr = document.createElement('tr');
      tr.innerHTML = '<td>' + k.replace(/_/g, ' ') + '</td><td>' + (decay[k] != null ? decay[k] : '—') + '</td>';
      dtbody.appendChild(tr);
    }});

    // Recent activity
    var activity = data.recent_activity || [];
    var abody = document.getElementById('activity-table');
    if (activity.length === 0) {{
      abody.innerHTML = '<tr><td colspan="4" style="color:#666">No activity yet</td></tr>';
    }} else {{
      abody.innerHTML = '';
      activity.forEach(function(a) {{
        var tr = document.createElement('tr');
        var ts = a.timestamp ? new Date(a.timestamp).toLocaleTimeString() : '—';
        tr.innerHTML = '<td>' + ts + '</td><td>' + (a.tenant_id || '—') + '</td>' +
                       '<td>' + (a.memory_type || '—') + '</td><td>' + (a.content || '').substring(0, 80) + '</td>';
        abody.appendChild(tr);
      }});
    }}
  }}

  // Render static data embedded by server, then poll for live updates
  render(DATA);

  setInterval(function() {{
    fetch('/api/status').then(function(r) {{ return r.json(); }})
      .then(render).catch(function() {{}});
  }}, 5000);
}})();
</script>
</body>
</html>"#
    )
}
