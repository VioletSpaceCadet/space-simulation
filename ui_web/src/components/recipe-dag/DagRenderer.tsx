import dagre from 'dagre';
import { useMemo } from 'react';

import { RECIPE_STATUS_COLORS, itemTypeColor } from '../../config/theme';
import type { ItemFlowStats, ModuleFlowStats } from '../../types';
import { displayName, formatQty } from '../../utils';
import type { GraphEdge, ItemNode, RecipeGraph, RecipeNode } from '../../utils/recipeGraph';

const ITEM_NODE_WIDTH = 80;
const ITEM_NODE_HEIGHT = 40;
const RECIPE_NODE_WIDTH = 140;
const RECIPE_NODE_HEIGHT = 32;

interface DagRendererProps {
  graph: RecipeGraph
  moduleFlowStats: Map<string, ModuleFlowStats>
  itemFlowStats: Map<string, ItemFlowStats>
  selectedNodeId: string | null
  onNodeSelect: (nodeId: string | null) => void
  onNodeHover: (nodeId: string | null, x: number, y: number) => void
  filter: 'all' | 'active'
}

interface LayoutNode {
  id: string
  x: number
  y: number
  width: number
  height: number
}

interface LayoutEdge {
  from: string
  to: string
  recipeId: string
}

interface LayoutResult {
  nodes: Map<string, LayoutNode>
  edges: LayoutEdge[]
  totalWidth: number
  totalHeight: number
}

function computeLayout(
  recipeNodes: Map<string, RecipeNode>,
  itemNodes: Map<string, ItemNode>,
  edges: GraphEdge[],
  filter: 'all' | 'active',
): LayoutResult {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'LR', nodesep: 40, ranksep: 80, marginx: 20, marginy: 20 });
  g.setDefaultEdgeLabel(() => ({}));

  // Determine which recipes to include
  const includedRecipes = new Set<string>();
  for (const [recipeId, recipe] of recipeNodes) {
    if (filter === 'all' || recipe.status === 'active') {
      includedRecipes.add(recipeId);
    }
  }

  // Determine which items are connected to included recipes
  const includedItems = new Set<string>();
  const filteredEdges: LayoutEdge[] = [];
  for (const edge of edges) {
    if (!includedRecipes.has(edge.recipeId)) { continue; }
    filteredEdges.push(edge);
    if (edge.from.startsWith('item:')) { includedItems.add(edge.from.replace('item:', '')); }
    if (edge.to.startsWith('item:')) { includedItems.add(edge.to.replace('item:', '')); }
  }

  for (const [itemId] of itemNodes) {
    if (!includedItems.has(itemId)) { continue; }
    g.setNode(`item:${itemId}`, { width: ITEM_NODE_WIDTH, height: ITEM_NODE_HEIGHT });
  }

  for (const recipeId of includedRecipes) {
    g.setNode(`recipe:${recipeId}`, { width: RECIPE_NODE_WIDTH, height: RECIPE_NODE_HEIGHT });
  }

  for (const edge of filteredEdges) {
    g.setEdge(edge.from, edge.to);
  }

  dagre.layout(g);

  const layoutNodes = new Map<string, LayoutNode>();
  let maxX = 0;
  let maxY = 0;

  for (const nodeId of g.nodes()) {
    const nodeData = g.node(nodeId);
    if (!nodeData) { continue; }
    const x = nodeData.x - nodeData.width / 2;
    const y = nodeData.y - nodeData.height / 2;
    layoutNodes.set(nodeId, {
      id: nodeId,
      x,
      y,
      width: nodeData.width as number,
      height: nodeData.height as number,
    });
    maxX = Math.max(maxX, x + (nodeData.width as number));
    maxY = Math.max(maxY, y + (nodeData.height as number));
  }

  return { nodes: layoutNodes, edges: filteredEdges, totalWidth: maxX, totalHeight: maxY };
}

function abbreviate(name: string): string {
  const parts = name.split('_').filter(Boolean);
  if (parts.length > 1) {
    return parts.map(p => p[0]).join('').slice(0, 3).toUpperCase();
  }
  if (name.length <= 3) { return name.toUpperCase(); }
  return name.slice(0, 3).toUpperCase();
}

/** Collect IDs of all nodes connected to selectedNodeId via edges */
function connectedNodes(selectedNodeId: string, edges: LayoutEdge[]): Set<string> {
  const connected = new Set<string>();
  connected.add(selectedNodeId);

  // Walk upstream and downstream
  let frontier = [selectedNodeId];
  const visited = new Set<string>(frontier);

  // Forward pass (downstream)
  while (frontier.length > 0) {
    const next: string[] = [];
    for (const nodeId of frontier) {
      for (const edge of edges) {
        if (edge.from === nodeId && !visited.has(edge.to)) {
          visited.add(edge.to);
          connected.add(edge.to);
          next.push(edge.to);
        }
      }
    }
    frontier = next;
  }

  // Backward pass (upstream)
  frontier = [selectedNodeId];
  const visitedBack = new Set<string>(frontier);
  while (frontier.length > 0) {
    const next: string[] = [];
    for (const nodeId of frontier) {
      for (const edge of edges) {
        if (edge.to === nodeId && !visitedBack.has(edge.from)) {
          visitedBack.add(edge.from);
          connected.add(edge.from);
          next.push(edge.from);
        }
      }
    }
    frontier = next;
  }

  return connected;
}

export function DagRenderer({
  graph,
  moduleFlowStats,
  itemFlowStats,
  selectedNodeId,
  onNodeSelect,
  onNodeHover,
  filter,
}: DagRendererProps) {
  // Structural key for memoizing layout
  const structureKey = useMemo(() => {
    const recipeIds = Array.from(graph.recipeNodes.keys()).sort().join(',');
    const itemIds = Array.from(graph.itemNodes.keys()).sort().join(',');
    const edgeIds = graph.edges.map(e => `${e.from}-${e.to}`).sort().join(',');
    return `${recipeIds}|${itemIds}|${edgeIds}|${filter}`;
  }, [graph.recipeNodes, graph.itemNodes, graph.edges, filter]);

  const layout = useMemo(
    () => computeLayout(graph.recipeNodes, graph.itemNodes, graph.edges, filter),
    // eslint-disable-next-line react-hooks/exhaustive-deps -- re-layout only on structural changes
    [structureKey],
  );

  if (layout.nodes.size === 0) {
    return (
      <div className="flex items-center justify-center h-full text-zinc-500 text-xs italic">
        no recipes available
      </div>
    );
  }

  const highlighted = selectedNodeId ? connectedNodes(selectedNodeId, layout.edges) : null;

  const padding = 16;
  const svgWidth = layout.totalWidth + padding * 2;
  const svgHeight = layout.totalHeight + padding * 2;

  return (
    <div style={{ position: 'relative', width: svgWidth, height: svgHeight }}>
      {/* Edge layer (SVG behind nodes) */}
      <svg
        style={{
          position: 'absolute',
          top: 0,
          left: 0,
          width: svgWidth,
          height: svgHeight,
          pointerEvents: 'none',
        }}
        width={svgWidth}
        height={svgHeight}
      >
        <defs>
          <marker
            id="arrowhead-active"
            markerWidth="6"
            markerHeight="4"
            refX="6"
            refY="2"
            orient="auto"
          >
            <polygon
              points="0 0, 6 2, 0 4"
              fill={RECIPE_STATUS_COLORS.active}
            />
          </marker>
          <marker
            id="arrowhead-available"
            markerWidth="6"
            markerHeight="4"
            refX="6"
            refY="2"
            orient="auto"
          >
            <polygon
              points="0 0, 6 2, 0 4"
              fill={RECIPE_STATUS_COLORS.available}
            />
          </marker>
        </defs>
        <g transform={`translate(${padding}, ${padding})`}>
          {layout.edges.map((edge, edgeIndex) => {
            const fromLayout = layout.nodes.get(edge.from);
            const toLayout = layout.nodes.get(edge.to);
            if (!fromLayout || !toLayout) { return null; }

            const x1 = fromLayout.x + fromLayout.width;
            const y1 = fromLayout.y + fromLayout.height / 2;
            const x2 = toLayout.x;
            const y2 = toLayout.y + toLayout.height / 2;

            const recipe = graph.recipeNodes.get(edge.recipeId);
            const isActive = recipe?.status === 'active';
            const strokeColor = isActive ? RECIPE_STATUS_COLORS.active : RECIPE_STATUS_COLORS.available;
            const strokeWidth = isActive ? 1.5 : 1;
            const markerId = isActive ? 'arrowhead-active' : 'arrowhead-available';

            const edgeHighlighted = !highlighted ||
              (highlighted.has(edge.from) && highlighted.has(edge.to));
            const opacity = edgeHighlighted ? 1 : 0.15;

            return (
              <line
                key={`${edge.from}-${edge.to}-${edgeIndex}`}
                x1={x1}
                y1={y1}
                x2={x2}
                y2={y2}
                stroke={strokeColor}
                strokeWidth={strokeWidth}
                opacity={opacity}
                markerEnd={`url(#${markerId})`}
                vectorEffect="non-scaling-stroke"
              />
            );
          })}
        </g>
      </svg>

      {/* Node layer */}
      <div
        style={{
          position: 'absolute',
          top: padding,
          left: padding,
        }}
      >
        {/* Item nodes */}
        {Array.from(graph.itemNodes.values()).map((item) => {
          const ln = layout.nodes.get(`item:${item.id}`);
          if (!ln) { return null; }

          const nodeId = `item:${item.id}`;
          const nodeHighlighted = !highlighted || highlighted.has(nodeId);
          const isSelected = selectedNodeId === nodeId;
          const flow = itemFlowStats.get(item.id);
          const displayQty = flow?.current_qty ?? item.inventory;

          return (
            <div
              key={nodeId}
              data-testid={`item-node-${item.id}`}
              role="button"
              tabIndex={0}
              style={{
                position: 'absolute',
                left: ln.x,
                top: ln.y,
                width: ln.width,
                height: ln.height,
                opacity: nodeHighlighted ? 1 : 0.2,
                cursor: 'pointer',
              }}
              className="flex flex-col items-center justify-center gap-0.5"
              onClick={() => onNodeSelect(isSelected ? null : nodeId)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onNodeSelect(isSelected ? null : nodeId);
                }
              }}
              onMouseEnter={(e) => {
                const rect = e.currentTarget.getBoundingClientRect();
                onNodeHover(nodeId, rect.left + rect.width / 2, rect.top);
              }}
              onMouseLeave={() => onNodeHover(null, 0, 0)}
            >
              <div
                className="w-7 h-7 rounded-full flex items-center justify-center shrink-0"
                style={{
                  background: itemTypeColor(item.type),
                  outline: isSelected ? '2px solid #fff' : 'none',
                  outlineOffset: '1px',
                }}
              >
                <span className="text-[10px] font-bold text-white leading-none">
                  {abbreviate(item.name)}
                </span>
              </div>
              <div className="text-[9px] text-zinc-500">
                {formatQty(displayQty)}
              </div>
            </div>
          );
        })}

        {/* Recipe nodes */}
        {Array.from(graph.recipeNodes.values()).map((recipe) => {
          const ln = layout.nodes.get(`recipe:${recipe.id}`);
          if (!ln) { return null; }

          const nodeId = `recipe:${recipe.id}`;
          const nodeHighlighted = !highlighted || highlighted.has(nodeId);
          const isSelected = selectedNodeId === nodeId;
          const statusColor = RECIPE_STATUS_COLORS[recipe.status] ?? RECIPE_STATUS_COLORS.available;

          // Find utilization from any module running this recipe
          let utilization = 0;
          for (const stats of moduleFlowStats.values()) {
            if (stats.recipe_id === recipe.id) {
              utilization = Math.max(utilization, stats.utilization_pct);
            }
          }

          return (
            <div
              key={nodeId}
              data-testid={`recipe-node-${recipe.id}`}
              role="button"
              tabIndex={0}
              style={{
                position: 'absolute',
                left: ln.x,
                top: ln.y,
                width: ln.width,
                height: ln.height,
                opacity: nodeHighlighted ? 1 : 0.2,
                cursor: 'pointer',
              }}
              className={`bg-zinc-800 border rounded-md flex items-center px-2 gap-1.5 ${
                isSelected ? 'border-white' : 'border-zinc-700'
              }`}
              onClick={() => onNodeSelect(isSelected ? null : nodeId)}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  e.preventDefault();
                  onNodeSelect(isSelected ? null : nodeId);
                }
              }}
              onMouseEnter={(e) => {
                const rect = e.currentTarget.getBoundingClientRect();
                onNodeHover(nodeId, rect.left + rect.width / 2, rect.top);
              }}
              onMouseLeave={() => onNodeHover(null, 0, 0)}
            >
              <div
                className="w-2 h-2 rounded-full shrink-0"
                data-testid={`recipe-status-${recipe.id}`}
                style={{ background: statusColor }}
              />
              <span className="text-[11px] text-zinc-200 truncate flex-1">
                {displayName(recipe.id)}
              </span>
              {/* Utilization bar at bottom */}
              <div
                className="absolute bottom-0 left-0 h-0.5 rounded-b-md"
                style={{
                  width: `${Math.min(utilization, 100)}%`,
                  background: statusColor,
                }}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
}
