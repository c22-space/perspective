import { useProcesses } from '../hooks';

function StatusDot({ status }: { status: string }) {
  const colors: Record<string, string> = {
    running: 'bg-emerald-400',
    idle: 'bg-zinc-500',
    error: 'bg-red-400',
    completed: 'bg-blue-400',
  };
  return <span className={`inline-block w-2 h-2 rounded-full ${colors[status] ?? 'bg-zinc-500'}`} />;
}

function SectionCard({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
      <h3 className="text-sm font-medium text-zinc-400 mb-4">{title}</h3>
      {children}
    </div>
  );
}

function KV({ label, value }: { label: string; value: string | number | null }) {
  return (
    <div className="flex justify-between py-1.5 border-b border-zinc-800/50 last:border-0">
      <span className="text-xs text-zinc-500">{label}</span>
      <span className="text-sm text-zinc-200">{value ?? '—'}</span>
    </div>
  );
}

export default function Processes() {
  const { data, error, loading } = useProcesses();

  if (loading) return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  if (error) return <div className="text-red-400">{error}</div>;
  if (!data) return null;

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold">Background Processes</h2>

      <div className="grid grid-cols-3 gap-4">
        <SectionCard title="Consolidation">
          <div className="flex items-center gap-2 mb-3">
            <StatusDot status={data.consolidation.status} />
            <span className="text-sm capitalize">{data.consolidation.status}</span>
          </div>
          <KV label="Last run" value={data.consolidation.last_run} />
          <KV label="Next run" value={data.consolidation.next_run} />
          <KV label="Items processed" value={data.consolidation.items_processed} />
          <KV label="Items deduped" value={data.consolidation.items_deduped} />
          <KV label="Items promoted" value={data.consolidation.items_promoted} />
        </SectionCard>

        <SectionCard title="Extraction Queue">
          <div className="flex items-center gap-2 mb-3">
            <StatusDot status={data.extraction.processing > 0 ? 'running' : 'idle'} />
            <span className="text-sm">
              {data.extraction.processing > 0 ? 'Processing' : 'Idle'}
            </span>
          </div>
          <KV label="Queue size" value={data.extraction.queue_size} />
          <KV label="Processing" value={data.extraction.processing} />
          <KV label="Completed" value={data.extraction.completed} />
        </SectionCard>

        <SectionCard title="Decay & GC">
          <div className="flex items-center gap-2 mb-3">
            <StatusDot status={data.decay.gc_candidates > 0 ? 'running' : 'idle'} />
            <span className="text-sm">
              {data.decay.gc_candidates > 0 ? `${data.decay.gc_candidates} candidates` : 'No candidates'}
            </span>
          </div>
          <KV label="GC candidates" value={data.decay.gc_candidates} />
          <KV label="Avg stability" value={data.decay.avg_stability.toFixed(2)} />
          <KV label="Last GC" value={data.decay.last_gc} />
        </SectionCard>
      </div>
    </div>
  );
}
