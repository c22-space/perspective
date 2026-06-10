import { useState } from 'react';
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

function TriggerButton({ onClick, label }: { onClick: () => void; label: string }) {
  return (
    <button
      onClick={onClick}
      className="px-3 py-1.5 text-xs font-medium bg-zinc-800 hover:bg-zinc-700 border border-zinc-700 hover:border-zinc-600 rounded-lg text-zinc-300 hover:text-zinc-100 transition-all duration-150 active:scale-95"
    >
      {label}
    </button>
  );
}

function TriggerFeedback({ status }: { status: string | null }) {
  if (!status) return null;
  const isError = status.startsWith('Error');
  return (
    <div className={`text-xs mt-2 px-2 py-1 rounded ${isError ? 'text-red-400 bg-red-400/10' : 'text-emerald-400 bg-emerald-400/10'}`}>
      {status}
    </div>
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
  const { data, error, loading, refresh } = useProcesses();
  const [consolidationStatus, setConsolidationStatus] = useState<string | null>(null);
  const [extractionStatus, setExtractionStatus] = useState<string | null>(null);
  const [decayStatus, setDecayStatus] = useState<string | null>(null);

  if (loading) return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  if (error) return <div className="text-red-400">{error}</div>;
  if (!data) return null;

  const qLen = data.extraction_queue.length;

  async function triggerConsolidation() {
    setConsolidationStatus('Running...');
    try {
      const resp = await fetch('/api/processes/trigger-consolidation', { method: 'POST' });
      const json = await resp.json();
      if (json.ok) {
        const reports = json.reports ?? [];
        setConsolidationStatus(`Done: ${reports.map((r: any) => `${r.tenant}: ${r.duplicates ?? 0} dupes`).join(', ') || 'no tenants'}`);
      } else {
        setConsolidationStatus(`Error: ${json.error ?? 'unknown'}`);
      }
    } catch (e: any) {
      setConsolidationStatus(`Error: ${e.message}`);
    }
    setTimeout(() => setConsolidationStatus(null), 5000);
    refresh();
  }

  async function triggerExtraction() {
    setExtractionStatus('Running...');
    try {
      const resp = await fetch('/api/processes/trigger-extraction', { method: 'POST' });
      const json = await resp.json();
      if (json.ok) {
        setExtractionStatus(`Done: extracted ${json.extracted} items`);
      } else {
        setExtractionStatus(`Error: ${json.error ?? 'unknown'}`);
      }
    } catch (e: any) {
      setExtractionStatus(`Error: ${e.message}`);
    }
    setTimeout(() => setExtractionStatus(null), 5000);
    refresh();
  }

  async function triggerDecay() {
    setDecayStatus('Running...');
    try {
      const resp = await fetch('/api/processes/trigger-decay', { method: 'POST' });
      const json = await resp.json();
      if (json.ok) {
        setDecayStatus('Done: decay tick completed');
      } else {
        setDecayStatus(`Error: ${json.error ?? 'unknown'}`);
      }
    } catch (e: any) {
      setDecayStatus(`Error: ${e.message}`);
    }
    setTimeout(() => setDecayStatus(null), 5000);
    refresh();
  }

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
          <div className="mt-3 pt-3 border-t border-zinc-800/50">
            <TriggerButton onClick={triggerConsolidation} label="Run Now" />
            <TriggerFeedback status={consolidationStatus} />
          </div>
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
          <div className="mt-3 pt-3 border-t border-zinc-800/50">
            <TriggerButton onClick={triggerExtraction} label="Run Now" />
            <TriggerFeedback status={extractionStatus} />
          </div>
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
          <div className="mt-3 pt-3 border-t border-zinc-800/50">
            <TriggerButton onClick={triggerDecay} label="Run Now" />
            <TriggerFeedback status={decayStatus} />
          </div>
        </SectionCard>
      </div>
    </div>
  );
}
