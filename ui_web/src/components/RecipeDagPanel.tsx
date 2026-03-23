import { useMemo, useState } from 'react';

import { useContent } from '../hooks/useContent';
import { useItemFlowStats } from '../hooks/useItemFlowStats';
import { useModuleFlowStats } from '../hooks/useModuleFlowStats';
import type { SimEvent, SimSnapshot, StationState } from '../types';
import { buildRecipeGraph } from '../utils/recipeGraph';

import { DagLegend } from './recipe-dag/DagLegend';
import { DagRenderer } from './recipe-dag/DagRenderer';
import { DagTooltip } from './recipe-dag/DagTooltip';

interface RecipeDagPanelProps {
  snapshot: SimSnapshot | null
  events: SimEvent[]
  currentTick: number
}

function computeModuleSummary(stations: Record<string, StationState>): string {
  let processors = 0;
  let activeProcessors = 0;
  let assemblers = 0;
  let activeAssemblers = 0;

  for (const station of Object.values(stations)) {
    for (const module of station.modules) {
      if (typeof module.kind_state === 'object') {
        if ('Processor' in module.kind_state) {
          processors++;
          if (module.enabled) { activeProcessors++; }
        } else if ('Assembler' in module.kind_state) {
          assemblers++;
          if (module.enabled) { activeAssemblers++; }
        }
      }
    }
  }

  const parts: string[] = [];
  if (processors > 0) { parts.push(`Proc: ${activeProcessors}/${processors}`); }
  if (assemblers > 0) { parts.push(`Asm: ${activeAssemblers}/${assemblers}`); }
  return parts.join(' \u00b7 ');
}

export function RecipeDagPanel({ snapshot, events, currentTick }: RecipeDagPanelProps) {
  const { content } = useContent();
  const [filter, setFilter] = useState<'all' | 'active'>('all');
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  const [hoveredNode, setHoveredNode] = useState<{ id: string; x: number; y: number } | null>(null);

  const minutesPerTick = content?.minutes_per_tick ?? 60;
  const moduleFlowStats = useModuleFlowStats(events, currentTick, minutesPerTick);
  const itemFlowStats = useItemFlowStats(snapshot, minutesPerTick);

  const recipes = content?.recipes;
  const graph = useMemo(() => {
    if (!recipes || !snapshot) { return null; }
    const unlockedTechs = snapshot.research?.unlocked ?? [];
    return buildRecipeGraph(recipes, snapshot.stations, unlockedTechs);
  }, [recipes, snapshot]);

  if (!snapshot || !content || !graph) {
    return (
      <div className="flex items-center justify-center h-full text-zinc-500 text-sm">
        Loading...
      </div>
    );
  }

  const moduleSummary = computeModuleSummary(snapshot.stations);

  return (
    <div className="flex flex-col h-full min-w-[20rem] overflow-hidden">
      {/* Toolbar */}
      <div className="flex items-center gap-3 px-3 py-1.5 border-b border-zinc-800 text-xs shrink-0">
        <button
          type="button"
          onClick={() => setFilter(f => f === 'all' ? 'active' : 'all')}
          className={`px-2 py-0.5 rounded cursor-pointer ${
            filter === 'active' ? 'bg-zinc-700 text-zinc-200' : 'bg-zinc-800 text-zinc-400'
          }`}
        >
          {filter === 'all' ? 'All' : 'Active'}
        </button>
        <div className="text-zinc-500 ml-auto">{moduleSummary}</div>
      </div>

      {/* DAG visualization */}
      <div className="relative flex-1 overflow-auto">
        <DagRenderer
          graph={graph}
          moduleFlowStats={moduleFlowStats}
          itemFlowStats={itemFlowStats}
          selectedNodeId={selectedNodeId}
          onNodeSelect={setSelectedNodeId}
          onNodeHover={(id, x, y) => setHoveredNode(id ? { id, x, y } : null)}
          filter={filter}
        />
        <DagLegend />
        {hoveredNode && (
          <DagTooltip
            nodeId={hoveredNode.id}
            x={hoveredNode.x}
            y={hoveredNode.y}
            graph={graph}
            moduleFlowStats={moduleFlowStats}
            itemFlowStats={itemFlowStats}
          />
        )}
      </div>
    </div>
  );
}
