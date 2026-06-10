import { useState } from 'react';
import { useSettings } from '../hooks';
import { api } from '../api';

function Toggle({
  label,
  value,
  onChange,
}: {
  label: string;
  value: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between py-2">
      <span className="text-sm text-zinc-300">{label}</span>
      <button
        type="button"
        onClick={() => onChange(!value)}
        className={`relative inline-flex h-5 w-9 items-center rounded-full transition-colors ${
          value ? 'bg-blue-600' : 'bg-zinc-700'
        }`}
      >
        <span
          className={`inline-block h-3.5 w-3.5 rounded-full bg-white transition-transform ${
            value ? 'translate-x-4.5' : 'translate-x-0.5'
          }`}
        />
      </button>
    </label>
  );
}

function NumberField({
  label,
  value,
  onChange,
  step = 0.01,
  min,
  max,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
  step?: number;
  min?: number;
  max?: number;
}) {
  return (
    <label className="flex items-center justify-between py-2">
      <span className="text-sm text-zinc-300">{label}</span>
      <input
        type="number"
        value={value}
        step={step}
        min={min}
        max={max}
        onChange={(e) => onChange(parseFloat(e.target.value) || 0)}
        className="w-24 bg-zinc-800 border border-zinc-700 rounded px-2 py-1 text-sm text-right text-zinc-200 font-mono focus:outline-none focus:border-blue-500"
      />
    </label>
  );
}

function Section({
  title,
  children,
}: {
  title: string;
  children: React.ReactNode;
}) {
  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-5">
      <h3 className="text-sm font-medium text-zinc-400 mb-3">{title}</h3>
      {children}
    </div>
  );
}

export default function Settings() {
  const { data, error, loading } = useSettings();
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);

  // Local editable state derived from server config
  const [decayEnabled, setDecayEnabled] = useState<boolean | null>(null);
  const [episodicLambda, setEpisodicLambda] = useState<number | null>(null);
  const [semanticLambda, setSemanticLambda] = useState<number | null>(null);
  const [proceduralLambda, setProceduralLambda] = useState<number | null>(null);
  const [learningRate, setLearningRate] = useState<number | null>(null);
  const [retrievalThreshold, setRetrievalThreshold] = useState<number | null>(null);
  const [gcThreshold, setGcThreshold] = useState<number | null>(null);
  const [extractionEnabled, setExtractionEnabled] = useState<boolean | null>(null);
  const [consolidationEnabled, setConsolidationEnabled] = useState<boolean | null>(null);

  // Initialize from server data
  if (data && decayEnabled === null) {
    setDecayEnabled(data.decay?.enabled === 'true');
    setEpisodicLambda(parseFloat(data.decay?.episodic_lambda || '0.1'));
    setSemanticLambda(parseFloat(data.decay?.semantic_lambda || '0.01'));
    setProceduralLambda(parseFloat(data.decay?.procedural_lambda || '0'));
    setLearningRate(parseFloat(data.decay?.learning_rate || '0.1'));
    setRetrievalThreshold(parseFloat(data.decay?.retrieval_threshold || '0.1'));
    setGcThreshold(parseFloat(data.decay?.gc_threshold || '0.01'));
    setExtractionEnabled(data.extraction?.enabled === 'true');
    setConsolidationEnabled(data.consolidation?.enabled === 'true');
  }

  if (loading) return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  if (error) return <div className="text-red-400">{error}</div>;
  if (!data) return null;

  const handleSave = async () => {
    setSaving(true);
    setSaveMsg(null);
    try {
      const patch: Record<string, unknown> = {};
      if (decayEnabled !== null) {
        patch.decay = {
          enabled: decayEnabled,
          episodic_lambda: episodicLambda,
          semantic_lambda: semanticLambda,
          procedural_lambda: proceduralLambda,
          learning_rate: learningRate,
          retrieval_threshold: retrievalThreshold,
          gc_threshold: gcThreshold,
        };
      }
      if (extractionEnabled !== null) {
        patch.extraction = { enabled: extractionEnabled };
      }
      if (consolidationEnabled !== null) {
        patch.consolidation = { enabled: consolidationEnabled };
      }
      const resp = await api.updateSettings(patch);
      if (resp.ok) {
        setSaveMsg('Settings saved. Changes are live.');
      } else {
        setSaveMsg(`Error: ${resp.error || 'unknown'}`);
      }
    } catch (e) {
      setSaveMsg(`Error: ${e}`);
    }
    setSaving(false);
  };

  const hasChanges =
    decayEnabled !== (data.decay?.enabled === 'true') ||
    extractionEnabled !== (data.extraction?.enabled === 'true') ||
    consolidationEnabled !== (data.consolidation?.enabled === 'true');

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h2 className="text-xl font-bold">Settings</h2>
        <div className="flex items-center gap-3">
          {saveMsg && (
            <span
              className={`text-sm ${
                saveMsg.startsWith('Error') ? 'text-red-400' : 'text-green-400'
              }`}
            >
              {saveMsg}
            </span>
          )}
          <button
            onClick={handleSave}
            disabled={saving || !hasChanges}
            className={`px-4 py-1.5 rounded-lg text-sm font-medium transition-colors ${
              hasChanges && !saving
                ? 'bg-blue-600 hover:bg-blue-500 text-white'
                : 'bg-zinc-800 text-zinc-500 cursor-not-allowed'
            }`}
          >
            {saving ? 'Saving...' : 'Save'}
          </button>
        </div>
      </div>

      <div className="grid grid-cols-2 gap-4">
        <Section title="Decay">
          <Toggle label="Enabled" value={!!decayEnabled} onChange={setDecayEnabled} />
          <NumberField
            label="Episodic lambda"
            value={episodicLambda ?? 0.1}
            onChange={setEpisodicLambda}
            step={0.01}
            min={0}
            max={10}
          />
          <NumberField
            label="Semantic lambda"
            value={semanticLambda ?? 0.01}
            onChange={setSemanticLambda}
            step={0.001}
            min={0}
            max={10}
          />
          <NumberField
            label="Procedural lambda"
            value={proceduralLambda ?? 0}
            onChange={setProceduralLambda}
            step={0.01}
            min={0}
            max={10}
          />
          <NumberField
            label="Learning rate"
            value={learningRate ?? 0.1}
            onChange={setLearningRate}
            step={0.01}
            min={0}
            max={1}
          />
          <NumberField
            label="Retrieval threshold"
            value={retrievalThreshold ?? 0.1}
            onChange={setRetrievalThreshold}
            step={0.01}
            min={0}
            max={1}
          />
          <NumberField
            label="GC threshold"
            value={gcThreshold ?? 0.01}
            onChange={setGcThreshold}
            step={0.001}
            min={0}
            max={1}
          />
        </Section>

        <Section title="Extraction">
          <Toggle
            label="Enabled"
            value={!!extractionEnabled}
            onChange={setExtractionEnabled}
          />
          <div className="py-2 text-xs text-zinc-500">
            Model: {data.extraction?.model ?? '—'}
          </div>
          <div className="py-2 text-xs text-zinc-500">
            Batch size: {data.extraction?.batch_size ?? '—'} tokens
          </div>
        </Section>

        <Section title="Consolidation">
          <Toggle
            label="Enabled"
            value={!!consolidationEnabled}
            onChange={setConsolidationEnabled}
          />
          <div className="py-2 text-xs text-zinc-500">
            Dedup threshold: {data.consolidation?.dedup_similarity_threshold ?? '—'}
          </div>
          <div className="py-2 text-xs text-zinc-500">
            Promotion access count: {data.consolidation?.promotion_access_count ?? '—'}
          </div>
        </Section>

        <Section title="Read-only Info">
          <div className="py-2 text-xs text-zinc-500">
            Embedding: {data.embedding?.model ?? '—'}
          </div>
          <div className="py-2 text-xs text-zinc-500">
            Data dir: {data.storage?.data_dir ?? '—'}
          </div>
          <div className="py-2 text-xs text-zinc-500">
            Retrieval budget: {data.retrieval?.default_budget ?? '—'}
          </div>
          <div className="py-2 text-xs text-zinc-500">
            Vector overfetch: {data.retrieval?.vector_overfetch ?? '—'}x
          </div>
        </Section>
      </div>
    </div>
  );
}
