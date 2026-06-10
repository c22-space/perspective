import { useState, useEffect, useRef } from 'react';

interface LogData {
  lines: string[];
  total: number;
  log_path: string;
}

export default function Logs() {
  const [data, setData] = useState<LogData | null>(null);
  const [loading, setLoading] = useState(true);
  const [filter, setFilter] = useState('');
  const [limit, setLimit] = useState(200);
  const [autoRefresh, setAutoRefresh] = useState(true);
  const outputRef = useRef<HTMLDivElement>(null);

  const fetchLogs = () => {
    const params = new URLSearchParams({ limit: String(limit) });
    if (filter) params.set('filter', filter);
    fetch(`/api/logs?${params}`)
      .then(r => r.json())
      .then(d => {
        // Handle error responses (e.g. {"error": "Not found"})
        if (d.error && !d.lines) {
          setData({ lines: [`Error: ${d.error}`], total: 0, log_path: '' });
        } else {
          setData(d);
        }
        setLoading(false);
      })
      .catch(() => setLoading(false));
  };

  useEffect(() => {
    fetchLogs();
  }, [filter, limit]);

  useEffect(() => {
    if (!autoRefresh) return;
    const id = setInterval(fetchLogs, 5000);
    return () => clearInterval(id);
  }, [autoRefresh, filter, limit]);

  // Auto-scroll to bottom on new logs
  useEffect(() => {
    if (outputRef.current) {
      outputRef.current.scrollTop = outputRef.current.scrollHeight;
    }
  }, [data]);

  const colorize = (line: string) => {
    if (line.includes('ERROR') || line.includes('error')) return 'text-red-400';
    if (line.includes('WARN') || line.includes('warn')) return 'text-yellow-400';
    if (line.includes('INFO') || line.includes('info')) return 'text-blue-400';
    return 'text-zinc-500';
  };

  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <h2 className="text-lg font-semibold">Server Logs</h2>
        <div className="flex items-center gap-3">
          <input
            type="text"
            placeholder="Filter logs..."
            value={filter}
            onChange={e => setFilter(e.target.value)}
            className="bg-zinc-900 border border-zinc-700 text-zinc-300 text-sm px-3 py-1.5 rounded-md w-48"
          />
          <select
            value={limit}
            onChange={e => setLimit(Number(e.target.value))}
            className="bg-zinc-900 border border-zinc-700 text-zinc-300 text-sm px-3 py-1.5 rounded-md"
          >
            <option value={50}>50 lines</option>
            <option value={100}>100 lines</option>
            <option value={200}>200 lines</option>
            <option value={500}>500 lines</option>
          </select>
          <button
            onClick={() => setAutoRefresh(!autoRefresh)}
            className={`text-sm px-3 py-1.5 rounded-md border ${
              autoRefresh
                ? 'bg-blue-600/20 border-blue-600 text-blue-400'
                : 'bg-zinc-800 border-zinc-700 text-zinc-400'
            }`}
          >
            {autoRefresh ? '● Live' : '○ Paused'}
          </button>
          <button
            onClick={fetchLogs}
            className="text-sm px-3 py-1.5 rounded-md bg-zinc-800 border border-zinc-700 text-zinc-400 hover:text-zinc-200"
          >
            Refresh
          </button>
        </div>
      </div>

      {data && (
        <p className="text-xs text-zinc-600">
          {data.total} total lines, showing {data.lines.length} | {data.log_path}
        </p>
      )}

      <div
        ref={outputRef}
        className="bg-zinc-950 border border-zinc-800 rounded-lg p-4 font-mono text-xs leading-5 overflow-auto"
        style={{ height: 'calc(100vh - 200px)' }}
      >
        {loading ? (
          <p className="text-zinc-600">Loading logs...</p>
        ) : data?.lines.length === 0 ? (
          <p className="text-zinc-600">No logs found{filter ? ` matching "${filter}"` : ''}</p>
        ) : (
          data?.lines.map((line, i) => (
            <div key={i} className={colorize(line)}>
              {line}
            </div>
          ))
        )}
      </div>
    </div>
  );
}
