import { useConfig } from '../hooks';

function KV({ label, value }: { label: string; value: string | number | null }) {
  return (
    <div className="flex justify-between py-2 border-b border-zinc-800/50 last:border-0">
      <span className="text-xs text-zinc-500">{label}</span>
      <span className="text-sm text-zinc-200 font-mono">{value ?? '—'}</span>
    </div>
  );
}

function Section({ title, entries }: { title: string; entries: [string, string][] }) {
  const sorted = [...entries].sort(([a], [b]) => a.localeCompare(b));
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
      <h3 className="text-sm font-medium text-zinc-400 mb-3">{title}</h3>
      {sorted.map(([k, v]) => (
        <KV key={k} label={k} value={v} />
      ))}
    </div>
  );
}

export default function Config() {
  const { data, error, loading } = useConfig();

  if (loading) return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  if (error) return <div className="text-red-400">{error}</div>;
  if (!data) return null;

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold">Configuration</h2>

      <div className="grid grid-cols-2 gap-4">
        <Section
          title="Storage"
          entries={Object.entries(data.storage)}
        />
        <Section
          title="Embedding"
          entries={Object.entries(data.embedding)}
        />
        <Section
          title="Decay"
          entries={Object.entries(data.decay)}
        />
        <Section
          title="Retrieval"
          entries={Object.entries(data.retrieval)}
        />
        <Section
          title="Consolidation"
          entries={Object.entries(data.consolidation)}
        />
        <Section
          title="Extraction"
          entries={Object.entries(data.extraction)}
        />
      </div>
    </div>
  );
}
