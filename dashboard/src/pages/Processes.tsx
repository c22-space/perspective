import { useProcesses } from '../hooks';

function StatusDot({ active }: { active: boolean }) {
  return (
    <span
      className={`inline-block w-2 h-2 rounded-full ${
        active ? 'bg-emerald-400' : 'bg-zinc-500'
      }`}
    />
  );
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

  const qLen = data.extraction_queue.length;

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold">Background Processes</h2>

      <div className="grid grid-cols-3 gap-4">
        <SectionCard title="Consolidation">
          <div className="flex items-center gap-2 mb-3">
            <StatusDot active={data.consolidation.running} />
            <span className="text-sm">
              {data.consolidation.running ? 'Running' : 'Idle'}
            </span>
          </div>
          <KV label="Last run" value={data.consolidation.last_run} />
          <KV label="Next run" value={data.consolidation.next_run} />
          <KV label="Items processed" value={data.consolidation.items_processed} />
          <KV label="Merges" value={data.consolidation.merges} />
          <KV label="Promotions" value={data.consolidation.promotions} />
        </SectionCard>

        <SectionCard title="Extraction Queue">
          <div className="flex items-center gap-2 mb-3">
            <StatusDot active={data.extraction_loop_active} />
            <span className="text-sm">
              {data.extraction_loop_active
                ? (qLen > 0 ? `${qLen} pending` : 'Active')
                : 'Inactive'}
            </span>
          </div>
          {qLen === 0 && data.extraction_loop_active && (
            <p className="text-zinc-600 text-xs py-4 text-center">No items in queue</p>
          )}
          {qLen === 0 && !data.extraction_loop_active && (
            <p className="text-zinc-600 text-xs py-4 text-center">Extraction loop not started</p>
          )}
        </SectionCard>

        <SectionCard title="Decay & GC">
          <div className="flex items-center gap-2 mb-3">
            <StatusDot active={data.decay_scheduler_active} />
            <span className="text-sm">
              {data.decay_scheduler_active
                ? (data.decay.gc_candidates > 0 ? `${data.decay.gc_candidates} candidates` : 'Scheduled')
                : 'Inactive'}
            </span>
          </div>
          <KV label="GC candidates" value={data.decay.gc_candidates} />
          <KV label="Items collected" value={data.decay.items_collected} />
          <KV label="Last GC" value={data.decay.last_gc_run} />
        </SectionCard>
      </div>
    </div>
  );
}
