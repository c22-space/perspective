import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer } from 'recharts';
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

  const eventColors: Record<string, string> = {
    store: 'bg-emerald-500/20 text-emerald-400',
    recall: 'bg-blue-500/20 text-blue-400',
    reflect: 'bg-amber-500/20 text-amber-400',
    delete: 'bg-red-500/20 text-red-400',
  };

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
        <ResponsiveContainer width="100%" height={200}>
          <BarChart data={typeData}>
            <XAxis dataKey="type" tick={{ fill: '#a1a1aa', fontSize: 12 }} axisLine={false} tickLine={false} />
            <YAxis tick={{ fill: '#a1a1aa', fontSize: 12 }} axisLine={false} tickLine={false} />
            <Tooltip
              contentStyle={{ background: '#18181b', border: '1px solid #27272a', borderRadius: 8, color: '#fff' }}
            />
            <Bar dataKey="count" fill="#3b82f6" radius={[6, 6, 0, 0]} />
          </BarChart>
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
            <div key={i} className="flex items-center gap-3 py-1.5 text-sm">
              <span className="text-xs text-zinc-600 w-16 shrink-0">{formatTime(ev.timestamp)}</span>
              <span
                className={`px-2 py-0.5 rounded text-xs font-medium ${
                  eventColors[ev.event_type] ?? 'bg-zinc-800 text-zinc-400'
                }`}
              >
                {ev.event_type}
              </span>
              {ev.memory_type && (
                <span className="text-xs text-zinc-500">{ev.memory_type}</span>
              )}
              <span className="text-zinc-400 truncate flex-1">
                {ev.details_json
                  ? (() => { try { return JSON.parse(ev.details_json).preview || ''; } catch { return ''; } })()
                  : ev.memory_id ?? ''}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
