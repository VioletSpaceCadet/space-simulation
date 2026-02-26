export type PanelId = 'map' | 'events' | 'asteroids' | 'fleet' | 'research' | 'economy'
export type LeafNode = { type: 'leaf'; panelId: PanelId }
export type GroupNode = {
  type: 'group'
  direction: 'horizontal' | 'vertical'
  children: LayoutNode[]
}
export type LayoutNode = LeafNode | GroupNode

export const ALL_PANELS: PanelId[] = ['map', 'events', 'asteroids', 'fleet', 'research', 'economy'];

export const PANEL_LABELS: Record<PanelId, string> = {
  map: 'Map',
  events: 'Events',
  asteroids: 'Asteroids',
  fleet: 'Fleet',
  research: 'Research',
  economy: 'Economy',
};

const VALID_PANEL_IDS = new Set<string>(ALL_PANELS);
const VALID_DIRECTIONS = new Set(['horizontal', 'vertical']);

export function buildDefaultLayout(panels: PanelId[]): GroupNode {
  return {
    type: 'group',
    direction: 'horizontal',
    children: panels.map((panelId) => ({ type: 'leaf', panelId })),
  };
}

export function findPanelIds(node: LayoutNode): PanelId[] {
  if (node.type === 'leaf') {return [node.panelId];}
  return node.children.flatMap(findPanelIds);
}

export function removePanel(node: LayoutNode, panelId: PanelId): LayoutNode {
  if (node.type === 'leaf') {return node;}

  let changed = false;
  const mapped: Array<LayoutNode | null> = node.children.map((child) => {
    if (child.type === 'leaf') {
      if (child.panelId === panelId) {
        changed = true;
        return null;
      }
      return child;
    }
    const result = removePanel(child, panelId);
    if (result !== child) {changed = true;}
    return result;
  });

  if (!changed) {return node;}

  const filtered = mapped.filter((child): child is LayoutNode => child !== null);
  if (filtered.length === 1) {return filtered[0];}

  return { ...node, children: filtered };
}

type Position = 'before' | 'after' | 'above' | 'below'

export function insertPanel(
  node: LayoutNode,
  panelId: PanelId,
  targetId: PanelId,
  position: Position,
): LayoutNode {
  const newLeaf: LeafNode = { type: 'leaf', panelId };
  return insertInto(node, newLeaf, targetId, position);
}

function insertInto(
  node: LayoutNode,
  newLeaf: LeafNode,
  targetId: PanelId,
  position: Position,
): LayoutNode {
  if (node.type === 'leaf') {
    if (node.panelId !== targetId) {return node;}
    // This leaf is the target — wrapping happens at the parent level
    // This case is reached only if the root itself is a leaf
    if (position === 'before' || position === 'after') {
      const children =
        position === 'before' ? [newLeaf, node] : [node, newLeaf];
      return { type: 'group', direction: 'horizontal', children };
    }
    const children =
      position === 'above' ? [newLeaf, node] : [node, newLeaf];
    return { type: 'group', direction: 'vertical', children };
  }

  // Group node: check if any direct child is the target
  const sameAxis =
    (node.direction === 'horizontal' && (position === 'before' || position === 'after')) ||
    (node.direction === 'vertical' && (position === 'above' || position === 'below'));

  if (sameAxis) {
    const targetIndex = node.children.findIndex(
      (child) => child.type === 'leaf' && child.panelId === targetId,
    );
    if (targetIndex !== -1) {
      const insertIndex =
        position === 'before' || position === 'above'
          ? targetIndex
          : targetIndex + 1;
      const newChildren = [...node.children];
      newChildren.splice(insertIndex, 0, newLeaf);
      return { ...node, children: newChildren };
    }
  }

  // Check if any direct child is the target and we need to wrap
  const crossAxis =
    (node.direction === 'horizontal' && (position === 'above' || position === 'below')) ||
    (node.direction === 'vertical' && (position === 'before' || position === 'after'));

  if (crossAxis) {
    const targetIndex = node.children.findIndex(
      (child) => child.type === 'leaf' && child.panelId === targetId,
    );
    if (targetIndex !== -1) {
      const target = node.children[targetIndex];
      const wrapDirection =
        position === 'above' || position === 'below' ? 'vertical' : 'horizontal';
      const children =
        position === 'above' || position === 'before'
          ? [newLeaf, target]
          : [target, newLeaf];
      const wrapper: GroupNode = { type: 'group', direction: wrapDirection, children };
      const newChildren = [...node.children];
      newChildren[targetIndex] = wrapper;
      return { ...node, children: newChildren };
    }
  }

  // Target is deeper — check children that are groups
  // If a child group has the matching direction for same-axis insert, recurse into it
  const newChildren = node.children.map((child) => {
    if (child.type === 'leaf') {return child;}

    // Check if the target is a direct child of this group and the group direction matches
    const childSameAxis =
      (child.direction === 'horizontal' && (position === 'before' || position === 'after')) ||
      (child.direction === 'vertical' && (position === 'above' || position === 'below'));

    if (childSameAxis) {
      const targetInChild = child.children.findIndex(
        (grandchild) => grandchild.type === 'leaf' && grandchild.panelId === targetId,
      );
      if (targetInChild !== -1) {
        const insertIndex =
          position === 'before' || position === 'above'
            ? targetInChild
            : targetInChild + 1;
        const updatedChildren = [...child.children];
        updatedChildren.splice(insertIndex, 0, newLeaf);
        return { ...child, children: updatedChildren };
      }
    }

    return insertInto(child, newLeaf, targetId, position);
  });

  return { ...node, children: newChildren };
}

export function movePanel(
  node: LayoutNode,
  panelId: PanelId,
  targetId: PanelId,
  position: Position,
): LayoutNode {
  const removed = removePanel(node, panelId);
  return insertPanel(removed, panelId, targetId, position);
}

/** Returns true if movePanel would produce a different layout. */
export function wouldMoveChange(
  layout: LayoutNode,
  sourceId: PanelId,
  targetId: PanelId,
  position: Position,
): boolean {
  const result = movePanel(layout, sourceId, targetId, position);
  return JSON.stringify(result) !== JSON.stringify(layout);
}

export function serializeLayout(node: LayoutNode): string {
  return JSON.stringify(node);
}

function validateNode(value: unknown): LayoutNode | null {
  if (typeof value !== 'object' || value === null) {return null;}

  const record = value as Record<string, unknown>;

  if (record.type === 'leaf') {
    if (typeof record.panelId !== 'string' || !VALID_PANEL_IDS.has(record.panelId)) {return null;}
    return { type: 'leaf', panelId: record.panelId as PanelId };
  }

  if (record.type === 'group') {
    if (typeof record.direction !== 'string' || !VALID_DIRECTIONS.has(record.direction)) {return null;}
    if (!Array.isArray(record.children)) {return null;}
    const children: LayoutNode[] = [];
    for (const child of record.children) {
      const validated = validateNode(child);
      if (validated === null) {return null;}
      children.push(validated);
    }
    return {
      type: 'group',
      direction: record.direction as 'horizontal' | 'vertical',
      children,
    };
  }

  return null;
}

export function deserializeLayout(json: string): LayoutNode | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    return null;
  }
  return validateNode(parsed);
}
