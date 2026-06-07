import { useState, useEffect, useCallback } from 'react';
import { api } from './api';

function usePolling<T>(fetcher: () => Promise<T>, intervalMs = 5000) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const result = await fetcher();
      setData(result);
      setError(null);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : 'Unknown error');
    } finally {
      setLoading(false);
    }
  }, [fetcher]);

  useEffect(() => {
    refresh();
    const id = setInterval(refresh, intervalMs);
    return () => clearInterval(id);
  }, [refresh, intervalMs]);

  return { data, error, loading, refresh };
}

export function useStatus() {
  return usePolling(() => api.getStatus());
}

export function useActivity(limit = 50) {
  return usePolling(() => api.getActivity(limit));
}

export function useProcesses() {
  return usePolling(() => api.getProcesses());
}

export function useGraph() {
  return usePolling(() => api.getGraph());
}

export function useMemories(q?: string) {
  return usePolling(() => api.getMemories(q));
}

export function useConfig() {
  return usePolling(() => api.getConfig(), 30000);
}
