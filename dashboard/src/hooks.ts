import { useState, useEffect, useRef } from 'react';
import { api } from './api';

function usePolling<T>(fetcher: () => Promise<T>, intervalMs = 5000) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const fetcherRef = useRef(fetcher);
  fetcherRef.current = fetcher;

  useEffect(() => {
    let alive = true;

    async function refresh() {
      try {
        const result = await fetcherRef.current();
        if (alive) { setData(result); setError(null); }
      } catch (e: unknown) {
        if (alive) setError(e instanceof Error ? e.message : 'Unknown error');
      } finally {
        if (alive) setLoading(false);
      }
    }

    refresh();
    const id = setInterval(refresh, intervalMs);
    return () => { alive = false; clearInterval(id); };
  }, [intervalMs]);

  return { data, error, loading };
}

function useOnce<T>(fetcher: () => Promise<T>) {
  const [data, setData] = useState<T | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    fetcher()
      .then((result) => { setData(result); setError(null); })
      .catch((e: unknown) => { setError(e instanceof Error ? e.message : 'Unknown error'); })
      .finally(() => setLoading(false));
  }, []);

  return { data, error, loading };
}

export function useStatus() {
  return usePolling(() => api.getStatus(), 10000);
}

export function useActivity(limit = 50) {
  return usePolling(() => api.getActivity(limit), 10000);
}

export function useProcesses() {
  return usePolling(() => api.getProcesses(), 10000);
}

export function useGraph() {
  return usePolling(() => api.getGraph(), 10000);
}

export function useMemories(q?: string) {
  return usePolling(() => api.getMemories(q), 10000);
}

export function useConfig() {
  return useOnce(() => api.getConfig());
}
