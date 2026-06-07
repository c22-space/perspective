#!/usr/bin/env python3
"""Perspective Dashboard Server — port 19177"""
import json
import os
import sys
import time
import traceback
from http.server import HTTPServer, SimpleHTTPRequestHandler
from pathlib import Path

# Add parent dir for imports
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

# Create engine once at startup (Tantivy index can only be opened once)
ENGINE = None
try:
    from perspective_python import PerspectiveEngine
    _data_dir = os.path.expanduser(os.environ.get('PERSPECTIVE_DATA_DIR', '~/.perspective/data'))
    ENGINE = PerspectiveEngine(_data_dir)
except Exception as e:
    print(f"Warning: Could not create engine: {e}")

DASHBOARD_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>Perspective Dashboard</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:-apple-system,BlinkMacSystemFont,'Segoe UI',Roboto,sans-serif;background:#0a0a0f;color:#e0e0e0;min-height:100vh}
.header{background:linear-gradient(135deg,#1a1a2e 0%,#16213e 100%);padding:24px 32px;border-bottom:1px solid #2a2a4a}
.header h1{font-size:28px;background:linear-gradient(135deg,#a78bfa,#60a5fa);-webkit-background-clip:text;-webkit-text-fill-color:transparent}
.header p{color:#888;margin-top:4px}
.container{max-width:1200px;margin:0 auto;padding:24px 32px}
.stats{display:grid;grid-template-columns:repeat(4,1fr);gap:16px;margin-bottom:32px}
.stat-card{background:#12121e;border:1px solid #2a2a4a;border-radius:12px;padding:20px}
.stat-card .label{font-size:12px;text-transform:uppercase;color:#888;letter-spacing:1px}
.stat-card .value{font-size:32px;font-weight:700;margin-top:8px}
.stat-card .value.purple{color:#a78bfa}.stat-card .value.blue{color:#60a5fa}
.stat-card .value.green{color:#34d399}.stat-card .value.amber{color:#fbbf24}
.section{background:#12121e;border:1px solid #2a2a4a;border-radius:12px;padding:24px;margin-bottom:24px}
.section h2{font-size:18px;margin-bottom:16px;color:#ccc}
table{width:100%;border-collapse:collapse}
th{text-align:left;padding:8px 12px;color:#888;font-size:12px;text-transform:uppercase;letter-spacing:1px;border-bottom:1px solid #2a2a4a}
td{padding:10px 12px;border-bottom:1px solid #1a1a2e;font-size:14px}
tr:hover td{background:#1a1a2e}
.badge{display:inline-block;padding:2px 8px;border-radius:6px;font-size:11px;font-weight:600}
.badge.episodic{background:#fbbf2433;color:#fbbf24}.badge.semantic{background:#60a5fa33;color:#60a5fa}
.badge.procedural{background:#34d39933;color:#34d399}
.health-dot{display:inline-block;width:8px;height:8px;border-radius:50%;margin-right:6px}
.health-dot.ok{background:#34d399}.health-dot.warn{background:#fbbf24}.health-dot.err{background:#ef4444}
#status{margin-top:8px;font-size:13px;color:#888}
.refresh-btn{background:#a78bfa;color:#000;border:none;padding:8px 16px;border-radius:8px;cursor:pointer;font-weight:600;font-size:13px;float:right}
.refresh-btn:hover{background:#8b5cf6}
</style>
</head>
<body>
<div class="header">
<h1>Perspective</h1>
<p>Memory Engine Dashboard</p>
<button class="refresh-btn" onclick="loadData()">Refresh</button>
</div>
<div class="container">
<div class="stats">
<div class="stat-card"><div class="label">Total Memories</div><div class="value purple" id="total">-</div></div>
<div class="stat-card"><div class="label">Episodic</div><div class="value amber" id="episodic">-</div></div>
<div class="stat-card"><div class="label">Semantic</div><div class="value blue" id="semantic">-</div></div>
<div class="stat-card"><div class="label">Procedural</div><div class="value green" id="procedural">-</div></div>
</div>
<div class="section">
<h2>Tenants</h2>
<table><thead><tr><th>Tenant</th><th>Memories</th><th>Status</th></tr></thead>
<tbody id="tenant-table"><tr><td colspan="3" style="color:#888">Loading...</td></tr></tbody></table>
</div>
<div class="section">
<h2>Recent Memories</h2>
<table><thead><tr><th>Content</th><th>Type</th><th>Created</th></tr></thead>
<tbody id="memory-table"><tr><td colspan="3" style="color:#888">Loading...</td></tr></tbody></table>
</div>
<div id="status"></div>
</div>
<script>
async function loadData(){
try{
const r=await fetch('/api/status');
const d=await r.json();
document.getElementById('total').textContent=d.total_memories||0;
document.getElementById('episodic').textContent=d.episodic_count||0;
document.getElementById('semantic').textContent=d.semantic_count||0;
document.getElementById('procedural').textContent=d.procedural_count||0;
const tb=document.getElementById('tenant-table');
tb.innerHTML='';
if(d.tenants&&d.tenants.length){
d.tenants.forEach(t=>{
tb.innerHTML+=`<tr><td>${t.name}</td><td>${t.memory_count}</td><td><span class="health-dot ok"></span>OK</td></tr>`;
});
}else{tb.innerHTML='<tr><td colspan="3" style="color:#888">No tenants</td></tr>';}
const mt=document.getElementById('memory-table');
mt.innerHTML='';
if(d.recent_memories&&d.recent_memories.length){
d.recent_memories.forEach(m=>{
mt.innerHTML+=`<tr><td>${m.content?m.content.substring(0,80)+'...':'N/A'}</td><td><span class="badge ${m.memory_type}">${m.memory_type}</span></td><td>${m.created_at||'-'}</td></tr>`;
});
}else{mt.innerHTML='<tr><td colspan="3" style="color:#888">No memories</td></tr>';}
document.getElementById('status').textContent='Last updated: '+new Date().toLocaleTimeString();
}catch(e){document.getElementById('status').textContent='Error: '+e.message;}
}
loadData();
setInterval(loadData,30000);
</script>
</body>
</html>"""


class DashboardHandler(SimpleHTTPRequestHandler):
    def log_message(self, format, *args):
        pass  # silent

    def do_GET(self):
        if self.path == '/' or self.path == '/dashboard':
            self.send_response(200)
            self.send_header('Content-Type', 'text/html')
            self.end_headers()
            self.wfile.write(DASHBOARD_HTML.encode())
        elif self.path == '/api/status':
            self._handle_status()
        elif self.path == '/api/activity':
            self._handle_activity()
        else:
            self.send_error(404)

    def _handle_activity(self):
        try:
            import sqlite3
            _data_dir = os.path.expanduser(os.environ.get('PERSPECTIVE_DATA_DIR', '~/.perspective/data'))
            db_path = os.path.join(_data_dir, 'activity.db')
            if not os.path.exists(db_path):
                data = {"events": [], "total_events": 0}
            else:
                conn = sqlite3.connect(db_path)
                cursor = conn.execute(
                    "SELECT id, timestamp, operation, memory_type, content, success "
                    "FROM events ORDER BY id DESC LIMIT 200"
                )
                events = []
                for row in cursor:
                    events.append({
                        "id": row[0],
                        "timestamp": row[1],
                        "operation": row[2],
                        "memory_type": row[3],
                        "content": row[4],
                        "success": bool(row[5]),
                    })
                total = conn.execute("SELECT COUNT(*) FROM events").fetchone()[0]
                conn.close()
                data = {"events": events, "total_events": total}
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())
        except Exception as e:
            data = {"error": str(e), "events": [], "total_events": 0}
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())

    def _handle_status(self):
        try:
            if ENGINE is None:
                raise RuntimeError("Engine not initialized")
            tenants = ENGINE.list_tenants()
            tenant_list = []
            total = 0
            episodic = 0
            semantic = 0
            procedural = 0
            for name in tenants:
                # No count_memories API, so recall with large budget and count results
                results = ENGINE.recall(name, "", budget=10000)
                count = len(results)
                tenant_list.append({"name": name, "memory_count": count})
                total += count
                for m in results:
                    if m.memory_type == "episodic":
                        episodic += 1
                    elif m.memory_type == "semantic":
                        semantic += 1
                    elif m.memory_type == "procedural":
                        procedural += 1
            # Recent memories across all tenants
            recent_list = []
            for name in tenants:
                recent = ENGINE.recall(name, "", budget=5)
                for m in recent:
                    recent_list.append({
                        "content": m.content,
                        "memory_type": m.memory_type,
                        "created_at": m.created_at,
                    })
            data = {
                "total_memories": total,
                "episodic_count": episodic,
                "semantic_count": semantic,
                "procedural_count": procedural,
                "tenants": tenant_list,
                "recent_memories": recent_list[:10],
            }
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())
        except Exception as e:
            data = {"error": str(e), "total_memories": 0, "tenants": [], "recent_memories": []}
            self.send_response(200)
            self.send_header('Content-Type', 'application/json')
            self.end_headers()
            self.wfile.write(json.dumps(data).encode())


if __name__ == '__main__':
    port = 19177
    server = HTTPServer(('0.0.0.0', port), DashboardHandler)
    print(f'Perspective Dashboard running on http://0.0.0.0:{port}')
    server.serve_forever()
