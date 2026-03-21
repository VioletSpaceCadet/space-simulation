import type { ResearchState, TechDef } from '../types';

export type NodeVisibility = 'unlocked' | 'researching' | 'locked' | 'mystery';
export type EdgeStyle = 'active' | 'dim' | 'fade';

export interface TreeNode {
  techId: string;
  name: string;
  state: NodeVisibility;
  domainRequirements: Record<string, number>;
  evidence: Record<string, number>;
}

export interface TreeEdge {
  from: string;
  to: string;
  style: EdgeStyle;
}

export interface TreeState {
  nodes: Map<string, TreeNode>;
  edges: TreeEdge[];
}

export const DOMAIN_COLORS: Record<string, string> = {
  Survey: '#5ca0c8',
  Materials: '#c89a4a',
  Manufacturing: '#4caf7d',
  Propulsion: '#a78bfa',
};

function getEvidence(techId: string, research: ResearchState): Record<string, number> {
  const domainProgress = research.evidence[techId];
  if (!domainProgress) {
    return {};
  }
  return { ...domainProgress.points };
}

function isResearching(techId: string, labAssignments: string[]): boolean {
  return labAssignments.includes(techId);
}

function prereqsMet(tech: TechDef, unlockedSet: Set<string>): boolean {
  return tech.prereqs.every(prereqId => unlockedSet.has(prereqId));
}

export function computeTreeState(
  techs: TechDef[],
  research: ResearchState,
  labAssignments: string[],
): TreeState {
  const unlockedSet = new Set(research.unlocked);
  const techById = new Map(techs.map(tech => [tech.id, tech]));

  // Collect sets by category
  const unlockedIds = new Set<string>();
  const researchingIds = new Set<string>();
  const lockedIds = new Set<string>();
  const mysteryIds = new Set<string>();

  // Pass 1: classify unlocked and researching techs
  for (const tech of techs) {
    if (unlockedSet.has(tech.id)) {
      unlockedIds.add(tech.id);
    } else if (isResearching(tech.id, labAssignments) && prereqsMet(tech, unlockedSet)) {
      researchingIds.add(tech.id);
    }
  }

  // Pass 2: find locked (direct children of unlocked or researching) and mystery (deeper)
  for (const tech of techs) {
    if (unlockedIds.has(tech.id) || researchingIds.has(tech.id)) {
      continue;
    }

    const hasVisibleParent = tech.prereqs.some(
      prereqId => unlockedIds.has(prereqId) || researchingIds.has(prereqId),
    );

    if (hasVisibleParent) {
      lockedIds.add(tech.id);
    }
  }

  // Pass 3: mystery = children of locked techs that are not themselves visible
  for (const tech of techs) {
    if (unlockedIds.has(tech.id) || researchingIds.has(tech.id) || lockedIds.has(tech.id)) {
      continue;
    }

    const hasLockedParent = tech.prereqs.some(prereqId => lockedIds.has(prereqId));
    if (hasLockedParent) {
      mysteryIds.add(tech.id);
    }
  }

  // Build nodes map
  const nodes = new Map<string, TreeNode>();

  const addNode = (techId: string, state: NodeVisibility) => {
    const tech = techById.get(techId);
    if (!tech) {
      return;
    }
    nodes.set(techId, {
      techId,
      name: tech.name,
      state,
      domainRequirements: { ...tech.domain_requirements },
      evidence: getEvidence(techId, research),
    });
  };

  for (const techId of unlockedIds) {
    addNode(techId, 'unlocked');
  }
  for (const techId of researchingIds) {
    addNode(techId, 'researching');
  }
  for (const techId of lockedIds) {
    addNode(techId, 'locked');
  }
  for (const techId of mysteryIds) {
    addNode(techId, 'mystery');
  }

  // Build edges between visible nodes only
  const edges: TreeEdge[] = [];
  const visibleIds = new Set([...unlockedIds, ...researchingIds, ...lockedIds, ...mysteryIds]);

  for (const techId of visibleIds) {
    const tech = techById.get(techId);
    if (!tech) {
      continue;
    }

    for (const prereqId of tech.prereqs) {
      // Only draw edges where the prerequisite (parent) is also visible
      if (!visibleIds.has(prereqId)) {
        continue;
      }

      const toNode = nodes.get(techId);
      const fromNode = nodes.get(prereqId);
      if (!toNode || !fromNode) {
        continue;
      }

      let style: EdgeStyle;
      if (toNode.state === 'mystery' || fromNode.state === 'mystery') {
        style = 'fade';
      } else if (toNode.state === 'locked') {
        style = 'dim';
      } else {
        // from unlocked/researching to unlocked/researching
        style = 'active';
      }

      edges.push({ from: prereqId, to: techId, style });
    }
  }

  return { nodes, edges };
}
