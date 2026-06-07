import { useConfig } from '../hooks';

function KV({ label, value }: { label: string; value: string | number | null }) {
  return (
    <div className="flex justify-between py-2 border-b border-zinc-800/50 last:border-0">
      <span className="text-xs text-zinc-500">{label}</span>
      <span className="text-sm text-zinc-200 font-mono">{value ?? '—'}</span>
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
        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">Storage Paths</h3>
          <KV label="Data directory" value={data.data_dir} />
          <KV label="Qdrant" value={data.qdrant_path} />
          <KV label="Tantivy" value={data.tantivy_path} />
          <KV label="Redb" value={data.redb_path} />
        </div>

        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">Embedding</h3>
          <KV label="Model" value={data.embedding_model} />
          <KV label="Dimension" value={data.embedding_dim} />
        </div>

        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">Decay (Ebbinghaus)</h3>
          <KV label="Episodic half-life" value={`${data.decay.episodic_half_life_days}d`} />
          <KV label="Semantic half-life" value={`${data.decay.semantic_half_life_days}d`} />
          <KV label="Procedural half-life" value={`${data.decay.procedural_half_life_days}d`} />
        </div>

        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">Consolidation</h3>
          <KV label="Interval" value={`${data.consolidation.interval_secs}s`} />
          <KV label="Batch size" value={data.consolidation.batch_size} />
          <KV label="Dedup threshold" value={data.consolidation.dedup_threshold} />
        </div>

        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5 col-span-2">
          <h3 className="text-sm font-medium text-zinc-400 mb-3">Extraction</h3>
          <KV label="Provider" value={data.extraction.provider} />
          <KV label="Model" value={data.extraction.model} />
          <KV label="Batch size" value={data.extraction.batch_size} />
        </div>
      </div>
    </div>
  );
}
