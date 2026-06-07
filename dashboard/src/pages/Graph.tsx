import { PieChart, Pie, Cell, Tooltip, ResponsiveContainer } from 'recharts';
import { useGraph } from '../hooks';

const COLORS = ['#3b82f6', '#10b981', '#f59e0b', '#ef4444', '#8b5cf6', '#06b6d4', '#ec4899', '#f97316'];

function Section({ title, data }: { title: string; data: Record<string, number> }) {
  const entries = Object.entries(data).filter(([, v]) => v > 0);
  const pieData = entries.map(([name, value]) => ({ name, value }));

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
      <h3 className="text-sm font-medium text-zinc-400 mb-3">{title}</h3>
      {entries.length === 0 ? (
        <p className="text-zinc-600 text-sm py-4 text-center">None</p>
      ) : (
        <div className="flex items-center gap-6">
          <ResponsiveContainer width={160} height={160}>
            <PieChart>
              <Pie data={pieData} dataKey="value" nameKey="name" cx="50%" cy="50%" outerRadius={60} innerRadius={30}>
                {pieData.map((_, i) => (
                  <Cell key={i} fill={COLORS[i % COLORS.length]} />
                ))}
              </Pie>
              <Tooltip
                contentStyle={{ background: '#18181b', border: '1px solid #27272a', borderRadius: 8, color: '#fff' }}
              />
            </PieChart>
          </ResponsiveContainer>
          <div className="space-y-1.5">
            {entries.map(([name, value], i) => (
              <div key={name} className="flex items-center gap-2 text-sm">
                <span className="w-2.5 h-2.5 rounded-full" style={{ background: COLORS[i % COLORS.length] }} />
                <span className="text-zinc-400">{name}</span>
                <span className="text-zinc-200 font-mono">{value}</span>
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

export default function Graph() {
  const { data, error, loading } = useGraph();

  if (loading) return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  if (error) return <div className="text-red-400">{error}</div>;
  if (!data) return null;

  const g = data.graph;

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold">Graph</h2>

      <div className="grid grid-cols-3 gap-4">
        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 text-center">
          <p className="text-xs text-zinc-500 uppercase tracking-wider">Total Nodes</p>
          <p className="text-3xl font-bold mt-1">{g.total_nodes}</p>
        </div>
        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 text-center">
          <p className="text-xs text-zinc-500 uppercase tracking-wider">Total Edges</p>
          <p className="text-3xl font-bold mt-1">{g.total_edges}</p>
        </div>
        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 text-center">
          <p className="text-xs text-zinc-500 uppercase tracking-wider">Communities</p>
          <p className="text-3xl font-bold mt-1">{g.communities}</p>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <Section title="Nodes by Type" data={g.node_types} />
        <Section title="Edges by Type" data={g.edge_types} />
      </div>
    </div>
  );
}
