import dagre from 'dagre';
import { useMemo } from 'react';

import type { ResearchState, TechDef } from '../types';

import { DOMAIN_COLORS, computeTreeState } from './techTree';
import type { EdgeStyle, NodeVisibility, TreeEdge, TreeNode } from './techTree';

interface TechTreeDAGProps {
  techs: TechDef[];
  research: ResearchState;
  labAssignments: string[];
}

const NODE_WIDTH = 196;
const NODE_BASE_HEIGHT = 36;
const NODE_DOMAIN_HEIGHT = 24;

function nodeHeight(node: TreeNode): number {
  if (node.state === 'mystery') {
    return NODE_BASE_HEIGHT;
  }
  const domainCount = Object.keys(node.domainRequirements).length;
  return NODE_BASE_HEIGHT + domainCount * NODE_DOMAIN_HEIGHT;
}

interface NodeStyle {
  background: string;
  border: string;
  borderStyle: string;
  nameColor: string;
  opacity: number;
}

function nodeStyle(state: NodeVisibility): NodeStyle {
  switch (state) {
    case 'unlocked':
      return {
        background: 'rgba(76,175,125,0.07)',
        border: 'rgba(76,175,125,0.35)',
        borderStyle: 'solid',
        nameColor: '#4caf7d',
        opacity: 1,
      };
    case 'researching':
      return {
        background: 'rgba(92,160,200,0.06)',
        border: 'rgba(92,160,200,0.30)',
        borderStyle: 'solid',
        nameColor: '#e0e2e8',
        opacity: 1,
      };
    case 'locked':
      return {
        background: '#13161e',
        border: '#2a2e38',
        borderStyle: 'dashed',
        nameColor: '#e0e2e8',
        opacity: 0.5,
      };
    case 'mystery':
      return {
        background: '#0e1018',
        border: '#1e2228',
        borderStyle: 'dashed',
        nameColor: '#2a2e38',
        opacity: 1,
      };
  }
}

function edgeStrokeProps(style: EdgeStyle): { stroke: string; strokeDasharray?: string } {
  switch (style) {
    case 'active':
      return { stroke: 'rgba(92,160,200,0.45)' };
    case 'dim':
      return { stroke: '#2a2e38' };
    case 'fade':
      return { stroke: '#1e2228', strokeDasharray: '4 3' };
  }
}

interface LayoutNode {
  node: TreeNode;
  x: number;
  y: number;
  width: number;
  height: number;
}

interface LayoutResult {
  layoutNodes: Map<string, LayoutNode>;
  edges: TreeEdge[];
  totalWidth: number;
  totalHeight: number;
}

function computeLayout(nodes: Map<string, TreeNode>, edges: TreeEdge[]): LayoutResult {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: 'TB', nodesep: 30, ranksep: 50 });
  g.setDefaultEdgeLabel(() => ({}));

  for (const [techId, node] of nodes) {
    const height = nodeHeight(node);
    g.setNode(techId, { width: NODE_WIDTH, height });
  }

  for (const edge of edges) {
    g.setEdge(edge.from, edge.to);
  }

  dagre.layout(g);

  const layoutNodes = new Map<string, LayoutNode>();
  let maxX = 0;
  let maxY = 0;

  for (const techId of g.nodes()) {
    const nodeData = g.node(techId);
    const treeNode = nodes.get(techId);
    if (!treeNode || !nodeData) {
      continue;
    }
    // dagre positions are center of node; convert to top-left
    const x = nodeData.x - nodeData.width / 2;
    const y = nodeData.y - nodeData.height / 2;
    layoutNodes.set(techId, {
      node: treeNode,
      x,
      y,
      width: nodeData.width as number,
      height: nodeData.height as number,
    });
    maxX = Math.max(maxX, x + (nodeData.width as number));
    maxY = Math.max(maxY, y + (nodeData.height as number));
  }

  return { layoutNodes, edges, totalWidth: maxX, totalHeight: maxY };
}

interface DomainBarProps {
  domain: string;
  required: number;
  evidence: number;
}

function DomainBar({ domain, required, evidence }: DomainBarProps) {
  const fill = Math.min(evidence / required, 1.0);
  const color = DOMAIN_COLORS[domain] ?? '#888888';

  return (
    <div style={{ marginBottom: 2 }}>
      <div
        style={{
          position: 'relative',
          height: 12,
          background: '#1a1d26',
          borderRadius: 2,
          overflow: 'hidden',
        }}
      >
        <div
          style={{
            position: 'absolute',
            left: 0,
            top: 0,
            bottom: 0,
            width: `${fill * 100}%`,
            background: color,
            opacity: 0.35,
          }}
        />
        <div
          style={{
            position: 'absolute',
            inset: 0,
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '0 4px',
            fontSize: 9,
            color: '#9aa0b0',
            lineHeight: 1,
          }}
        >
          <span>{domain}</span>
          <span>
            {Math.round(evidence)}/{required}
          </span>
        </div>
      </div>
    </div>
  );
}

interface NodeCardProps {
  layoutNode: LayoutNode;
}

function NodeCard({ layoutNode }: NodeCardProps) {
  const { node, x, y, width, height } = layoutNode;
  const style = nodeStyle(node.state);

  return (
    <div
      style={{
        position: 'absolute',
        left: x,
        top: y,
        width,
        height,
        background: style.background,
        border: `1px ${style.borderStyle} ${style.border}`,
        borderRadius: 4,
        opacity: style.opacity,
        padding: '6px 8px',
        boxSizing: 'border-box',
        overflow: 'hidden',
      }}
    >
      {node.state === 'mystery' ? (
        <div
          style={{
            fontSize: 11,
            color: style.nameColor,
            fontWeight: 500,
            display: 'flex',
            alignItems: 'center',
            height: '100%',
          }}
        >
          ???
        </div>
      ) : (
        <>
          <div
            style={{
              fontSize: 11,
              color: style.nameColor,
              fontWeight: 500,
              marginBottom: 4,
              whiteSpace: 'nowrap',
              overflow: 'hidden',
              textOverflow: 'ellipsis',
            }}
          >
            {node.name}
          </div>
          {Object.entries(node.domainRequirements).map(([domain, required]) => (
            <DomainBar
              key={domain}
              domain={domain}
              required={required}
              evidence={node.evidence[domain] ?? 0}
            />
          ))}
        </>
      )}
    </div>
  );
}

interface EdgeLineProps {
  edge: TreeEdge;
  layoutNodes: Map<string, LayoutNode>;
}

function EdgeLine({ edge, layoutNodes }: EdgeLineProps) {
  const fromLayout = layoutNodes.get(edge.from);
  const toLayout = layoutNodes.get(edge.to);
  if (!fromLayout || !toLayout) {
    return null;
  }

  // Connect bottom-center of source to top-center of target
  const x1 = fromLayout.x + fromLayout.width / 2;
  const y1 = fromLayout.y + fromLayout.height;
  const x2 = toLayout.x + toLayout.width / 2;
  const y2 = toLayout.y;

  const strokeProps = edgeStrokeProps(edge.style);

  return (
    <line
      x1={x1}
      y1={y1}
      x2={x2}
      y2={y2}
      stroke={strokeProps.stroke}
      strokeDasharray={strokeProps.strokeDasharray}
      strokeWidth={1.5}
      vectorEffect="non-scaling-stroke"
    />
  );
}

export function TechTreeDAG({ techs, research, labAssignments }: TechTreeDAGProps) {
  const treeState = useMemo(
    () => computeTreeState(techs, research, labAssignments),

    [techs, research, labAssignments],
  );

  const layout = useMemo(
    () => computeLayout(treeState.nodes, treeState.edges),
    [treeState.nodes, treeState.edges],
  );

  if (treeState.nodes.size === 0) {
    return (
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          height: '100%',
          color: '#3a3e4a',
          fontSize: 12,
          fontStyle: 'italic',
        }}
      >
        no research activity
      </div>
    );
  }

  const padding = 16;
  const svgWidth = layout.totalWidth + padding * 2;
  const svgHeight = layout.totalHeight + padding * 2;

  return (
    <div
      style={{
        position: 'relative',
        overflow: 'auto',
        width: '100%',
        height: '100%',
      }}
    >
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
        <g transform={`translate(${padding}, ${padding})`}>
          {layout.edges.map((edge) => (
            <EdgeLine key={`${edge.from}-${edge.to}`} edge={edge} layoutNodes={layout.layoutNodes} />
          ))}
        </g>
      </svg>

      {/* Node layer */}
      <div
        style={{
          position: 'relative',
          width: svgWidth,
          height: svgHeight,
        }}
      >
        <div
          style={{
            position: 'absolute',
            top: padding,
            left: padding,
          }}
        >
          {[...layout.layoutNodes.values()].map((layoutNode) => (
            <NodeCard key={layoutNode.node.techId} layoutNode={layoutNode} />
          ))}
        </div>
      </div>
    </div>
  );
}
