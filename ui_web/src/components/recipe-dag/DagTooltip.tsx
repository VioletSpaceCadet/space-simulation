import { ITEM_TYPE_COLORS, RECIPE_STATUS_COLORS } from '../../config/theme';
import type { ItemFlowStats, ModuleFlowStats } from '../../types';
import { displayName, formatQty } from '../../utils';
import type { RecipeGraph } from '../../utils/recipeGraph';

interface DagTooltipProps {
  nodeId: string | null
  x: number
  y: number
  graph: RecipeGraph
  moduleFlowStats: Map<string, ModuleFlowStats>
  itemFlowStats: Map<string, ItemFlowStats>
}

function trendArrow(trend: 'rising' | 'falling' | 'stable'): string {
  switch (trend) {
    case 'rising': return '\u2191';
    case 'falling': return '\u2193';
    case 'stable': return '\u2192';
  }
}

export function DagTooltip({ nodeId, x, y, graph, moduleFlowStats, itemFlowStats }: DagTooltipProps) {
  if (!nodeId) { return null; }

  if (nodeId.startsWith('item:')) {
    const itemId = nodeId.replace('item:', '');
    const item = graph.itemNodes.get(itemId);
    if (!item) { return null; }

    const flow = itemFlowStats.get(itemId);
    const typeBg = ITEM_TYPE_COLORS[item.type] ?? '#888';

    // Find recipes that consume/produce this item
    const consumedBy: string[] = [];
    const producedBy: string[] = [];
    for (const edge of graph.edges) {
      if (edge.from === nodeId) { consumedBy.push(edge.recipeId); }
      if (edge.to === nodeId) { producedBy.push(edge.recipeId); }
    }

    return (
      <div
        style={{
          position: 'fixed',
          left: x,
          top: y - 8,
          transform: 'translate(-50%, -100%)',
          zIndex: 50,
        }}
        className="bg-zinc-900 border border-zinc-700 rounded px-3 py-2 text-[10px] shadow-lg pointer-events-none max-w-[200px]"
      >
        <div className="flex items-center gap-1.5 mb-1">
          <span className="font-bold text-zinc-200">{displayName(item.name)}</span>
          <span
            className="px-1 rounded text-[9px] text-white"
            style={{ background: typeBg }}
          >
            {item.type}
          </span>
        </div>
        <div className="text-zinc-400">
          Stock: {formatQty(flow?.current_qty ?? item.inventory)}
          {flow && (
            <span className="ml-1.5">
              {trendArrow(flow.trend)} {flow.delta_per_hour >= 0 ? '+' : ''}{formatQty(flow.delta_per_hour)}/hr
            </span>
          )}
        </div>
        {producedBy.length > 0 && (
          <div className="text-zinc-500 mt-1">
            Produced by: {[...new Set(producedBy)].map(displayName).join(', ')}
          </div>
        )}
        {consumedBy.length > 0 && (
          <div className="text-zinc-500 mt-0.5">
            Consumed by: {[...new Set(consumedBy)].map(displayName).join(', ')}
          </div>
        )}
      </div>
    );
  }

  if (nodeId.startsWith('recipe:')) {
    const recipeId = nodeId.replace('recipe:', '');
    const recipe = graph.recipeNodes.get(recipeId);
    if (!recipe) { return null; }

    const statusColor = RECIPE_STATUS_COLORS[recipe.status] ?? RECIPE_STATUS_COLORS.available;

    // Aggregate stats across all modules running this recipe
    let totalThroughput = 0;
    let maxUtilization = 0;
    let stallReason: string | null = null;
    for (const stats of moduleFlowStats.values()) {
      if (stats.recipe_id === recipeId) {
        totalThroughput += stats.throughput_per_hour;
        maxUtilization = Math.max(maxUtilization, stats.utilization_pct);
        if (stats.stall_reason) { stallReason = stats.stall_reason; }
      }
    }

    return (
      <div
        style={{
          position: 'fixed',
          left: x,
          top: y - 8,
          transform: 'translate(-50%, -100%)',
          zIndex: 50,
        }}
        className="bg-zinc-900 border border-zinc-700 rounded px-3 py-2 text-[10px] shadow-lg pointer-events-none max-w-[220px]"
      >
        <div className="flex items-center gap-1.5 mb-1">
          <span className="font-bold text-zinc-200">{displayName(recipeId)}</span>
          <span
            className="px-1 rounded text-[9px] text-white"
            style={{ background: statusColor }}
          >
            {recipe.status}
          </span>
        </div>
        <div className="text-zinc-400 mb-1">
          {recipe.inputs.map((input, index) => (
            <div key={index}>In: {displayName(input.itemId)} x{input.amount}{input.unit === 'kg' ? ' kg' : ''}</div>
          ))}
          {recipe.outputs.map((output, index) => (
            <div key={index}>Out: {displayName(output.itemId)}</div>
          ))}
        </div>
        {totalThroughput > 0 && (
          <div className="text-zinc-400">
            Throughput: {formatQty(totalThroughput)}/hr
          </div>
        )}
        {maxUtilization > 0 && (
          <div className="flex items-center gap-1 mt-1">
            <span className="text-zinc-500">Util:</span>
            <div className="flex-1 h-1.5 bg-zinc-700 rounded overflow-hidden">
              <div
                className="h-full rounded"
                style={{
                  width: `${Math.min(maxUtilization, 100)}%`,
                  background: statusColor,
                }}
              />
            </div>
            <span className="text-zinc-400">{Math.round(maxUtilization)}%</span>
          </div>
        )}
        {stallReason && (
          <div className="text-amber-400 mt-1">Stalled: {stallReason}</div>
        )}
      </div>
    );
  }

  return null;
}
