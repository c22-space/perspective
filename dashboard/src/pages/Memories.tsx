import { useState } from 'react';
import { useMemories } from '../hooks';

function MemoryCard({ mem }: { mem: { id: string; content: string; memory_type: string; tags: string[]; created_at: string; importance?: number; stability?: number } }) {
  const typeColors: Record<string, string> = {
    episodic: 'bg-amber-500/15 text-amber-400',
    semantic: 'bg-blue-500/15 text-blue-400',
    procedural: 'bg-emerald-500/15 text-emerald-400',
  };

  return (
    <div className="bg-zinc-900 border border-zinc-800 rounded-lg p-3">
      <div className="flex items-center gap-2 mb-2">
        <span className={`px-2 py-0.5 rounded text-xs font-medium ${typeColors[mem.memory_type] ?? 'bg-zinc-800 text-zinc-400'}`}>
          {mem.memory_type}
        </span>
        {mem.importance !== undefined && mem.importance !== null && (
          <span className="text-xs text-zinc-600 font-mono">importance: {mem.importance.toFixed(2)}</span>
        )}
        {mem.stability !== undefined && mem.stability !== null && (
          <span className="text-xs text-zinc-600 font-mono">stability: {mem.stability.toFixed(2)}</span>
        )}
        <span className="text-xs text-zinc-600 ml-auto">{new Date(mem.created_at).toLocaleDateString()}</span>
      </div>
      <p className="text-sm text-zinc-300 leading-relaxed">{mem.content}</p>
      {mem.tags.length > 0 && (
        <div className="flex gap-1 mt-2 flex-wrap">
          {mem.tags.map((t) => (
            <span key={t} className="px-1.5 py-0.5 rounded bg-zinc-800 text-xs text-zinc-500">
              {t}
            </span>
          ))}
        </div>
      )}
    </div>
  );
}

export default function Memories() {
  const [query, setQuery] = useState('');
  const [search, setSearch] = useState('');
  const { data, error, loading } = useMemories(search);

  const handleSearch = () => {
    setSearch(query);
  };

  return (
    <div className="space-y-6">
      <h2 className="text-xl font-bold">Memories</h2>

      {/* Search bar */}
      <div className="flex gap-2">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => e.key === 'Enter' && handleSearch()}
          placeholder="Search memories..."
          className="flex-1 bg-zinc-900 border border-zinc-800 rounded-lg px-4 py-2.5 text-sm text-zinc-200 placeholder-zinc-600 focus:outline-none focus:border-zinc-600"
        />
        <button
          onClick={handleSearch}
          className="px-4 py-2.5 bg-blue-600 hover:bg-blue-500 text-white text-sm rounded-lg transition-colors"
        >
          Search
        </button>
      </div>

      {loading && <div className="text-zinc-500 animate-pulse">Loading...</div>}
      {error && <div className="text-red-400">{error}</div>}

      <div className="space-y-2">
        {data?.memories.length === 0 && (
          <p className="text-zinc-600 text-sm py-8 text-center">
            {search ? 'No memories match your search' : 'No memories stored yet'}
          </p>
        )}
        {data?.memories.map((mem) => (
          <MemoryCard key={mem.id} mem={mem} />
        ))}
      </div>

      {data && data.total > 50 && (
        <p className="text-xs text-zinc-600 text-center">
          Showing {Math.min(data.memories.length, 50)} of {data.total}
        </p>
      )}
    </div>
  );
}
