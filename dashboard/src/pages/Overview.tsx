import { useState } from 'react';
import { useStatus, useActivity, useGraph } from '../hooks';

function formatDuration(secs: number) {
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  const s = Math.floor(secs % 60);
  if (h > 0) return `${h}h ${m}m`;
  if (m > 0) return `${m}m ${s}s`;
  return `${s}s`;
}

function formatTime(iso: string) {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString();
  } catch {
    return iso;
  }
}

function StatCard({ label, value, sub }: { label: string; value: string | number; sub?: string }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
      <p className="text-xs text-zinc-500 uppercase tracking-wider">{label}</p>
      <p className="text-2xl font-bold mt-1">{value}</p>
      {sub && <p className="text-xs text-zinc-600 mt-1">{sub}</p>}
    </div>
  );
}

function ActivityItem({ event }: { event: { timestamp: string; operation: string; memory_type: string | null; memory_id: string | null; content: string | null; details_json: string | null } }) {
  const [expanded, setExpanded] = useState(false);
  const details = (() => {
    if (!event.details_json) return null;
    try { return JSON.parse(event.details_json); } catch { return null; }
  })();

  const eventColors: Record<string, string> = {
    store: 'bg-emerald-500/20 text-emerald-400',
    recall: 'bg-blue-500/20 text-blue-400',
    reflect: 'bg-amber-500/20 text-amber-400',
    delete: 'bg-red-500/20 text-red-400',
    extraction: 'bg-purple-500/20 text-purple-400',
    decay: 'bg-zinc-500/20 text-zinc-400',
  };

  return (
    <div
      className="py-1.5 text-sm cursor-pointer hover:bg-zinc-800/50 rounded px-2 -mx-2 transition-colors"
      onClick={() => setExpanded(!expanded)}
    >
      <div className="flex items-center gap-3">
        <span className="text-xs text-zinc-600 w-16 shrink-0">{formatTime(event.timestamp)}</span>
        <span
          className={`px-2 py-0.5 rounded text-xs font-medium ${
            eventColors[event.operation] ?? 'bg-zinc-800 text-zinc-400'
          }`}
        >
          {event.operation}
        </span>
        {event.memory_type && (
          <span className="text-xs text-zinc-500">{event.memory_type}</span>
        )}
        <span className="text-zinc-400 truncate flex-1">
          {details?.query ?? details?.content ?? details?.preview ?? event.content ?? event.memory_id ?? ''}
        </span>
        {details && (
          <span className="text-zinc-600 text-xs shrink-0">{expanded ? '▾' : '▸'}</span>
        )}
      </div>
      {expanded && details && (
        <div className="ml-19 mt-2 p-3 bg-zinc-800/50 rounded-lg text-xs space-y-1.5 border border-zinc-700/50">
          {details.query && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Query:</span>
              <span className="text-zinc-300 font-mono">{details.query}</span>
            </div>
          )}
          {details.result_count !== undefined && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Results:</span>
              <span className="text-zinc-300">{details.result_count}</span>
            </div>
          )}
          {details.budget && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Budget:</span>
              <span className="text-zinc-300">{details.budget}</span>
            </div>
          )}
          {details.content && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Content:</span>
              <span className="text-zinc-300">{details.content}</span>
            </div>
          )}
          {details.memory_type && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Type:</span>
              <span className="text-zinc-300">{details.memory_type}</span>
            </div>
          )}
          {details.tags && details.tags.length > 0 && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Tags:</span>
              <span className="text-zinc-300">{details.tags.join(', ')}</span>
            </div>
          )}
          {details.results && details.results.length > 0 && (
            <div className="space-y-1.5">
              <span className="text-zinc-500 text-xs">Results ({details.results.length}):</span>
              {details.results.map((r: {id: string; content: string; type: string}, i: number) => (
                <div key={i} className="ml-2 p-2 bg-zinc-900/50 rounded border border-zinc-700/30">
                  <div className="flex items-center gap-2 mb-1">
                    <span className="px-1.5 py-0.5 rounded text-[10px] font-medium bg-blue-500/20 text-blue-400">{r.type}</span>
                    <span className="text-zinc-600 text-[10px]">{r.id.slice(0, 8)}</span>
                  </div>
                  <p className="text-zinc-300 text-xs">{r.content}</p>
                </div>
              ))}
            </div>
          )}
          {details.fact_count !== undefined && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Facts extracted:</span>
              <span className="text-zinc-300">{details.fact_count}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

export default function Overview() {
  const { data: status, error: statusErr, loading: statusLoading } = useStatus();
  const { data: activity, error: actErr } = useActivity(30);
  const { data: graph } = useGraph();
  const [activityFilter, setActivityFilter] = useState<string>('all');

  const typeData = status
    ? [
        { type: 'Episodic', count: status.memory_types.episodic, color: '#3b82f6' },
        { type: 'Semantic', count: status.memory_types.semantic, color: '#10b981' },
        { type: 'Procedural', count: status.memory_types.procedural, color: '#f59e0b' },
      ]
    : [];

  const totalTyped = typeData.reduce((sum, d) => sum + d.count, 0);

  const events = activity?.events ?? [];
  const filteredEvents = activityFilter === 'all'
    ? events
    : events.filter(ev => ev.operation === activityFilter);
  const operationTypes = [...new Set(events.map(ev => ev.operation))].sort();

  if (statusLoading) {
    return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  }

  if (statusErr) {
    return (
      <div className="text-center py-20">
        <p className="text-red-400 text-lg">Engine offline</p>
        <p className="text-zinc-500 text-sm mt-2">{statusErr}</p>
        <p className="text-zinc-600 text-xs mt-4">Start perspective server to view dashboard</p>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold">Overview</h2>

      {/* Stat cards */}
      <div className="grid grid-cols-6 gap-4">
        <StatCard label="Health" value={status?.health ?? '—'} />
        <StatCard
          label="Uptime"
          value={status ? formatDuration(status.uptime_secs) : '—'}
        />
        <StatCard
          label="Memories"
          value={status?.total_memories ?? 0}
          sub={`${status?.tenant_count ?? 0} tenants`}
        />
        <StatCard
          label="Nodes"
          value={graph?.graph?.total_nodes ?? 0}
          sub={graph ? `${graph.graph.node_types.memory_ref} mem, ${graph.graph.node_types.entity} ent` : undefined}
        />
        <StatCard
          label="Edges"
          value={graph?.graph?.total_edges ?? 0}
          sub={graph ? `${graph.graph.edge_types.Semantic ?? 0} sem, ${graph.graph.edge_types.Entity ?? 0} ent` : undefined}
        />
        <StatCard
          label="GC Candidates"
          value={status?.gc_candidates ?? 0}
        />
      </div>

      {/* Memory type distribution */}
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">Memory Distribution</h3>
        {totalTyped === 0 ? (
          <p className="text-zinc-600 text-sm py-4 text-center">
            {status?.total_memories ? `${status.total_memories} memories` : 'No memories yet'}
          </p>
        ) : (
          <div className="space-y-3">
            {typeData.map(d => (
              <div key={d.type} className="flex items-center gap-3">
                <span className="text-xs text-zinc-500 w-20 shrink-0">{d.type}</span>
                <div className="flex-1 bg-zinc-800 rounded-full h-3 overflow-hidden">
                  <div
                    className="h-full rounded-full transition-all"
                    style={{
                      width: `${(d.count / totalTyped) * 100}%`,
                      backgroundColor: d.color,
                    }}
                  />
                </div>
                <span className="text-xs text-zinc-400 w-16 text-right">{d.count} ({totalTyped > 0 ? ((d.count / totalTyped) * 100).toFixed(0) : 0}%)</span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Activity feed */}
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <div className="flex items-center justify-between mb-3">
          <h3 className="text-sm font-medium text-zinc-400">Recent Activity</h3>
          <select
            value={activityFilter}
            onChange={e => setActivityFilter(e.target.value)}
            className="bg-zinc-800 border border-zinc-700 text-zinc-300 text-xs rounded px-2 py-1 focus:outline-none focus:border-zinc-500"
          >
            <option value="all">All types</option>
            {operationTypes.map(op => (
              <option key={op} value={op}>{op}</option>
            ))}
          </select>
        </div>
        {actErr && <p className="text-red-400 text-xs mb-2">{actErr}</p>}
        <div className="space-y-1 max-h-96 overflow-auto">
          {filteredEvents.length === 0 && (
            <p className="text-zinc-600 text-sm py-4 text-center">
              {events.length === 0 ? 'No activity yet' : 'No matching events'}
            </p>
          )}
          {filteredEvents.map((ev, i) => (
            <ActivityItem key={i} event={ev} />
          ))}
        </div>
      </div>
    </div>
  );
}
