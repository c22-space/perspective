const API_BASE = import.meta.env.VITE_API_URL || '';

export interface StatusResponse {
  health: string;
  uptime_secs: number;
  total_memories: number;
  tenant_count: number;
  memory_types: { episodic: number; semantic: number; procedural: number };
  gc_candidates: number;
  decay_config: Record<string, number>;
  recent_activity: ActivityEvent[];
}

export interface ActivityEvent {
  tenant: string;
  operation: string;
  memory_type: string | null;
  memory_id: string | null;
  timestamp: string;
  content: string | null;
  success: boolean;
  details_json: string | null;
}

export interface ProcessStatus {
  consolidation: {
    running: boolean;
    last_run: string | null;
    next_run: string | null;
    items_processed: number;
    merges: number;
    promotions: number;
  };
  decay: {
    gc_candidates: number;
    last_gc_run: string | null;
    items_collected: number;
    avg_stability_episodic: number | null;
    avg_stability_semantic: number | null;
  };
  extraction_queue: unknown[];
  consolidation_history: unknown[];
  extraction_loop_active: boolean;
  decay_scheduler_active: boolean;
}

export interface GraphStats {
  graph: {
    total_nodes: number;
    total_edges: number;
    communities: number;
    avg_connectivity: number;
    node_types: Record<string, number>;
    edge_types: Record<string, number>;
    recent_edges: unknown[];
  };
}

export interface Memory {
  id: string;
  content: string;
  memory_type: string;
  tags: string[];
  created_at: string;
  updated_at: string;
  importance?: number;
  stability?: number;
  access_count?: number;
}

export interface MemoriesResponse {
  memories: Memory[];
  total: number;
}

export interface SettingsResponse {
  storage: Record<string, string>;
  embedding: Record<string, string>;
  decay: Record<string, string>;
  retrieval: Record<string, string>;
  consolidation: Record<string, string>;
  extraction: Record<string, string>;
}

export interface GraphNode {
  id: string;
  label: string;
  type: string;
  group: number;
}

export interface GraphLink {
  source: string;
  target: string;
  type: string;
  weight: number;
}

async function fetchJson<T>(path: string): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`);
  if (!res.ok) throw new Error(`API error: ${res.status} ${res.statusText}`);
  return res.json();
}

export const api = {
  getStatus: () => fetchJson<StatusResponse>('/api/status'),
  getActivity: (limit = 50) =>
    fetchJson<{ events: ActivityEvent[] }>(`/api/activity?limit=${limit}`),
  getProcesses: () => fetchJson<ProcessStatus>('/api/processes'),
  getGraph: () => fetchJson<GraphStats>('/api/graph'),
  getFullGraph: () =>
    fetchJson<{ nodes: GraphNode[]; links: GraphLink[] }>('/api/graph/full'),
  getMemories: (q?: string, limit = 50) => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (q) params.set('q', q);
    return fetchJson<MemoriesResponse>(`/api/memories?${params}`);
  },
  getSettings: () => fetchJson<SettingsResponse>('/api/config'),
  updateSettings: (patch: Record<string, unknown>) =>
    fetch(`${API_BASE}/api/settings`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(patch),
    }).then(r => r.json()),
};
