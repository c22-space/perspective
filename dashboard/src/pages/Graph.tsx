import { useState, useCallback, useRef, useEffect } from 'react';
import ForceGraph2D from 'react-force-graph-2d';
import { useGraph, useFullGraph } from '../hooks';
import type { GraphNode, GraphLink } from '../api';

const GROUP_COLORS: Record<number, string> = {
  1: '#3b82f6', // memory_ref - blue
  2: '#10b981', // entity - green
  3: '#f59e0b', // concept - amber
};

const EDGE_COLORS: Record<string, string> = {
  Semantic: '#3b82f6',
  Temporal: '#6366f1',
  Entity: '#10b981',
  Causes: '#ef4444',
  Enables: '#f59e0b',
  Supports: '#22c55e',
  Contradicts: '#dc2626',
  PromotedFrom: '#8b5cf6',
};

interface GraphData {
  nodes: GraphNode[];
  links: GraphLink[];
}

interface VizNode extends GraphNode {
  x?: number;
  y?: number;
  vx?: number;
  vy?: number;
}

function StatsBar({ data }: { data: { graph: { total_nodes: number; total_edges: number; communities: number; avg_connectivity: number; node_types: Record<string, number>; edge_types: Record<string, number> } } }) {
  const g = data.graph;
  return (
    <div className="grid grid-cols-4 gap-4">
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4 text-center">
        <p className="text-xs text-zinc-500 uppercase tracking-wider">Nodes</p>
        <p className="text-2xl font-bold mt-1">{g.total_nodes}</p>
      </div>
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4 text-center">
        <p className="text-xs text-zinc-500 uppercase tracking-wider">Edges</p>
        <p className="text-2xl font-bold mt-1">{g.total_edges}</p>
      </div>
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4 text-center">
        <p className="text-xs text-zinc-500 uppercase tracking-wider">Avg Connectivity</p>
        <p className="text-2xl font-bold mt-1">{g.avg_connectivity.toFixed(1)}</p>
      </div>
      <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4 text-center">
        <p className="text-xs text-zinc-500 uppercase tracking-wider">Communities</p>
        <p className="text-2xl font-bold mt-1">{g.communities}</p>
      </div>
    </div>
  );
}

function Legend() {
  return (
    <div className="flex items-center gap-4 text-xs">
      <span className="text-zinc-500 uppercase tracking-wider">Nodes:</span>
      {Object.entries({ memory: GROUP_COLORS[1], entity: GROUP_COLORS[2], concept: GROUP_COLORS[3] }).map(([label, color]) => (
        <div key={label} className="flex items-center gap-1.5">
          <span className="w-2.5 h-2.5 rounded-full" style={{ background: color }} />
          <span className="text-zinc-400">{label}</span>
        </div>
      ))}
      <span className="text-zinc-600 mx-2">|</span>
      <span className="text-zinc-500 uppercase tracking-wider">Edges:</span>
      {Object.entries(EDGE_COLORS).slice(0, 4).map(([label, color]) => (
        <div key={label} className="flex items-center gap-1.5">
          <span className="w-4 h-0.5" style={{ background: color }} />
          <span className="text-zinc-400">{label}</span>
        </div>
      ))}
    </div>
  );
}

export default function Graph() {
  const { data: statsData, error: statsErr, loading: statsLoading } = useGraph();
  const { data: fullData } = useFullGraph();
  const [tooltip, setTooltip] = useState<{ x: number; y: number; node: GraphNode } | null>(null);
  const [selectedNode, setSelectedNode] = useState<GraphNode | null>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState({ width: 800, height: 500 });

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const obs = new ResizeObserver(entries => {
      for (const entry of entries) {
        setDimensions({ width: entry.contentRect.width, height: Math.max(400, entry.contentRect.width * 0.6) });
      }
    });
    obs.observe(el);
    return () => obs.disconnect();
  }, []);

  const graphData: GraphData = fullData ?? { nodes: [], links: [] };

  const nodeCanvasObject = useCallback((node: VizNode, ctx: CanvasRenderingContext2D) => {
    const size = 6;
    ctx.beginPath();
    ctx.arc(node.x ?? 0, node.y ?? 0, size, 0, 2 * Math.PI);
    ctx.fillStyle = GROUP_COLORS[node.group] ?? '#6b7280';
    ctx.fill();

    // Label
    const label = node.label?.length > 20 ? node.label.slice(0, 20) + '...' : (node.label ?? '');
    if (label) {
      ctx.font = '3px Inter, sans-serif';
      ctx.textAlign = 'center';
      ctx.textBaseline = 'top';
      ctx.fillStyle = '#a1a1aa';
      ctx.fillText(label, node.x ?? 0, (node.y ?? 0) + size + 1);
    }
  }, []);

  const nodePointerAreaPaint = useCallback((node: VizNode, color: string, ctx: CanvasRenderingContext2D) => {
    const size = 8;
    ctx.beginPath();
    ctx.arc(node.x ?? 0, node.y ?? 0, size, 0, 2 * Math.PI);
    ctx.fillStyle = color;
    ctx.fill();
  }, []);

  const handleNodeHover = useCallback((node: VizNode | null) => {
    if (node) {
      setTooltip({ x: 0, y: 0, node });
    } else {
      setTooltip(null);
    }
  }, []);

  const handleNodeClick = useCallback((node: VizNode) => {
    setSelectedNode(prev => prev?.id === node.id ? null : node);
  }, []);

  if (statsLoading) return <div className="text-zinc-500 animate-pulse">Loading...</div>;
  if (statsErr) return <div className="text-red-400">{statsErr}</div>;
  if (!statsData) return null;

  return (
    <div className="space-y-4">
      <h2 className="text-xl font-bold">Graph</h2>

      <StatsBar data={statsData} />

      <Legend />

      {/* Interactive graph */}
      <div ref={containerRef} className="bg-zinc-900 border border-zinc-800 rounded-xl overflow-hidden relative">
        {graphData.nodes.length === 0 ? (
          <div className="text-zinc-600 text-sm py-20 text-center">No graph data yet. Memories and entities will appear as they are stored.</div>
        ) : (
          <ForceGraph2D
            graphData={graphData}
            width={dimensions.width}
            height={dimensions.height}
            nodeCanvasObject={nodeCanvasObject}
            nodePointerAreaPaint={nodePointerAreaPaint}
            onNodeHover={handleNodeHover}
            onNodeClick={handleNodeClick}
            nodeRelSize={1}
            linkColor={(link: GraphLink) => EDGE_COLORS[link.type] ?? '#4b5563'}
            linkWidth={0.5}
            linkDirectionalParticles={1}
            linkDirectionalParticleWidth={1.5}
            linkDirectionalParticleSpeed={0.005}
            linkDirectionalParticleColor={(link: GraphLink) => EDGE_COLORS[link.type] ?? '#6b7280'}
            backgroundColor="#09090b"
            warmupTicks={50}
            cooldownTicks={100}
            d3VelocityDecay={0.3}
          />
        )}

        {/* Tooltip on hover */}
        {tooltip && (
          <div className="absolute top-3 right-3 bg-zinc-800 border border-zinc-700 rounded-lg p-3 text-xs space-y-1 pointer-events-none z-10 max-w-xs">
            <div className="flex items-center gap-2">
              <span className="w-2.5 h-2.5 rounded-full" style={{ background: GROUP_COLORS[tooltip.node.group] ?? '#6b7280' }} />
              <span className="text-zinc-400 uppercase tracking-wider">{tooltip.node.type}</span>
            </div>
            <p className="text-zinc-200 font-medium">{tooltip.node.label}</p>
            <p className="text-zinc-500 text-[10px]">{tooltip.node.id.slice(0, 12)}...</p>
          </div>
        )}
      </div>

      {/* Selected node detail */}
      {selectedNode && (
        <div className="bg-zinc-900 border border-zinc-800 rounded-xl p-4 space-y-2">
          <div className="flex items-center justify-between">
            <h3 className="text-sm font-medium text-zinc-400">Selected Node</h3>
            <button onClick={() => setSelectedNode(null)} className="text-zinc-600 hover:text-zinc-400 text-xs">close</button>
          </div>
          <div className="flex items-center gap-2">
            <span className="w-3 h-3 rounded-full" style={{ background: GROUP_COLORS[selectedNode.group] ?? '#6b7280' }} />
            <span className="text-zinc-400 uppercase tracking-wider text-xs">{selectedNode.type}</span>
          </div>
          <p className="text-zinc-200">{selectedNode.label}</p>
          <p className="text-zinc-500 text-xs font-mono">{selectedNode.id}</p>
          <div className="text-xs text-zinc-500">
            {graphData.links.filter((l: GraphLink) => l.source === selectedNode.id || l.target === selectedNode.id).length} connections
          </div>
        </div>
      )}
    </div>
  );
}
