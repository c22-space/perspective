const API_BASE = import.meta.env.VITE_API_URL || '';

export interface StatusResponse {
  uptime_secs: number;
  total_memories: number;
  memory_counts: { episodic: number; semantic: number; procedural: number };
  gc_candidates: number;
  extraction_queue_size: number;
  tenants: string[];
}

export interface ActivityEvent {
  id: number;
  tenant: string;
  event_type: string;
  memory_type: string | null;
  memory_id: string | null;
  timestamp: string;
  details_json: string | null;
}

export interface ProcessStatus {
  consolidation: {
    status: string;
    last_run: string | null;
    next_run: string | null;
    items_processed: number;
    items_deduped: number;
    items_promoted: number;
  };
  extraction: {
    queue_size: number;
    processing: number;
    completed: number;
  };
  decay: {
    gc_candidates: number;
    avg_stability: number;
    last_gc: string | null;
  };
}

export interface GraphStats {
  nodes: { total: number; by_type: Record<string, number> };
  edges: { total: number; by_type: Record<string, number> };
  communities: number;
}

export interface Memory {
  id: string;
  content: string;
  memory_type: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  score?: number;
}

export interface MemoriesResponse {
  memories: Memory[];
  total: number;
}

export interface ConfigResponse {
  data_dir: string;
  qdrant_path: string;
  tantivy_path: string;
  redb_path: string;
  embedding_model: string;
  embedding_dim: number;
  decay: {
    episodic_half_life_days: number;
    semantic_half_life_days: number;
    procedural_half_life_days: number;
  };
  consolidation: {
    interval_secs: number;
    batch_size: number;
    dedup_threshold: number;
  };
  extraction: {
    provider: string;
    model: string;
    batch_size: number;
  };
}

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`);
  if (!res.ok) throw new Error(`API error: ${res.status} ${res.statusText}`);
  return res.json();
}

export const api = {
  getStatus: () => fetchJson<StatusResponse>('/api/status'),
  getActivity: (limit = 50) => fetchJson<ActivityEvent[]>(`/api/activity?limit=${limit}`),
  getProcesses: () => fetchJson<ProcessStatus>('/api/processes'),
  getGraph: () => fetchJson<GraphStats>('/api/graph'),
  getMemories: (q?: string, limit = 50) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (q) params.set('q', q);
    return fetchJson<MemoriesResponse>(`/api/memories?${params}`);
  },
  getConfig: () => fetchJson<ConfigResponse>('/api/config'),
};
