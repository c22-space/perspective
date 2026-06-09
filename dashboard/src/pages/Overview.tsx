import { useState } from 'react';
import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer } from 'recharts';
import { useStatus, useActivity } from '../hooks';

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

function ActivityItem({ event }: { event: { timestamp: string; event_type: string; memory_type: string | null; memory_id: string | null; details_json: string | null } }) {
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
    extract: 'bg-purple-500/20 text-purple-400',
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
            eventColors[event.event_type] ?? 'bg-zinc-800 text-zinc-400'
          }`}
        >
          {event.event_type}
        </span>
        {event.memory_type && (
          <span className="text-xs text-zinc-500">{event.memory_type}</span>
        )}
        <span className="text-zinc-400 truncate flex-1">
          {details?.query ?? details?.content ?? details?.preview ?? event.memory_id ?? ''}
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
          {details.entities && details.entities.length > 0 && (
            <div className="flex gap-2">
              <span className="text-zinc-500 shrink-0">Entities:</span>
              <span className="text-zinc-300">{details.entities.join(', ')}</span>
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

  const typeData = status
    ? [
        { type: 'Episodic', count: status.memory_types.episodic },
        { type: 'Semantic', count: status.memory_types.semantic },
        { type: 'Procedural', count: status.memory_types.procedural },
      ]
    : [];

  const events = activity?.events ?? [];

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
      <div className="grid grid-cols-4 gap-4">
        <StatCard label="Health" value={status?.health ?? '—'} />
        <StatCard
          label="Uptime"
          value={status ? formatDuration(status.uptime_secs) : '—'}
        />
        <StatCard
          label="Total Memories"
          value={status?.total_memories ?? 0}
          sub={`${status?.tenant_count ?? 0} tenants`}
        />
        <StatCard
          label="GC Candidates"
          value={status?.gc_candidates ?? 0}
        />
      </div>

      {/* Memory type distribution */}
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">Memory Distribution</h3>
        <ResponsiveContainer width="100%" height={220}>
          <PieChart>
            <Pie
              data={typeData}
              dataKey="count"
              nameKey="type"
              cx="50%"
              cy="50%"
              outerRadius={80}
              innerRadius={40}
              paddingAngle={2}
              label={({ name, percent }: { name?: string; percent?: number }) => `${name ?? ''} ${((percent ?? 0) * 100).toFixed(0)}%`}
            >
              {typeData.map((_, i) => (
                <Cell key={i} fill={['#3b82f6', '#10b981', '#f59e0b'][i % 3]} />
              ))}
            </Pie>
            <Tooltip
              contentStyle={{ background: '#18181b', border: '1px solid #27272a', borderRadius: 8, color: '#fff' }}
            />
          </PieChart>
        </ResponsiveContainer>
      </div>

      {/* Activity feed */}
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4">
        <h3 className="text-sm font-medium text-zinc-400 mb-3">Recent Activity</h3>
        {actErr && <p className="text-red-400 text-xs mb-2">{actErr}</p>}
        <div className="space-y-1 max-h-96 overflow-auto">
          {events.length === 0 && (
            <p className="text-zinc-600 text-sm py-4 text-center">No activity yet</p>
          )}
          {events.map((ev, i) => (
            <ActivityItem key={i} event={ev} />
          ))}
        </div>
      </div>
    </div>
  );
}
