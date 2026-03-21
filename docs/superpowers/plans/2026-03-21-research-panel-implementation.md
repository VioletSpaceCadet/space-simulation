# Research Panel Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat-list research panel with a DAG-based tech tree, data pool rates, and lab status sections.

**Architecture:** New `useContent` hook fetches tech definitions from `GET /api/v1/content`. Pure visibility logic computes node states from `ResearchState` + `TechDef[]`. Dagre computes layout positions. HTML nodes + SVG edge overlay render the tree. Three-section panel: Data Pool, Tech Tree, Lab Status.

**Tech Stack:** React 19, TypeScript 5, dagre (layout), Tailwind v4, vitest

**Spec:** `docs/superpowers/specs/2026-03-21-research-panel-redesign.md`
**Mockup:** `docs/superpowers/specs/2026-03-21-research-panel-mockup.html`

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `ui_web/src/types.ts` | Modify | Add `TechDef`, `ContentResponse`, `LabRateInfo` types |
| `ui_web/src/api.ts` | Modify | Add `fetchContent()` function |
| `ui_web/src/hooks/useContent.ts` | Create | Hook to fetch + cache content from `/api/v1/content` |
| `ui_web/src/hooks/useContent.test.ts` | Create | Tests for content hook |
| `ui_web/src/components/techTree.ts` | Create | Pure logic: visibility, node states, edge states from ResearchState + TechDef[] |
| `ui_web/src/components/techTree.test.ts` | Create | Tests for visibility/state logic |
| `ui_web/src/components/TechTreeDAG.tsx` | Create | React component: dagre layout + HTML nodes + SVG edges |
| `ui_web/src/components/TechTreeDAG.test.tsx` | Create | Tests for DAG rendering |
| `ui_web/src/components/DataPoolSection.tsx` | Create | Data pool 2-column grid with rates |
| `ui_web/src/components/DataPoolSection.test.tsx` | Create | Tests for data pool section |
| `ui_web/src/components/LabStatusSection.tsx` | Create | Lab status compact rows |
| `ui_web/src/components/LabStatusSection.test.tsx` | Create | Tests for lab status section |
| `ui_web/src/components/ResearchPanel.tsx` | Modify | Replace with 3-section layout using new components |
| `ui_web/src/components/ResearchPanel.test.tsx` | Modify | Update tests for new panel |

## Domain Color Constants

Used across multiple components. Define once:

```typescript
export const DOMAIN_COLORS: Record<string, string> = {
  Survey: '#5ca0c8',
  Materials: '#c89a4a',
  Manufacturing: '#4caf7d',
  Propulsion: '#a78bfa',
};
```

---

### Task 1: Add Types and Content API

**Files:**
- Modify: `ui_web/src/types.ts`
- Modify: `ui_web/src/api.ts`
- Modify: `ui_web/src/api.test.ts`

- [ ] **Step 1: Add TechDef and ContentResponse types to types.ts**

```typescript
export interface TechEffect {
  type: string
  sigma?: number
}

export interface TechDef {
  id: string
  name: string
  prereqs: string[]
  domain_requirements: Record<string, number>
  accepted_data: string[]
  difficulty: number
  effects: TechEffect[]
}

export interface LabRateInfo {
  station_id: string
  module_id: string
  module_name: string
  assigned_tech: string | null
  domain: string
  points_per_hour: number
  starved: boolean
  enabled: boolean
}

export interface ContentResponse {
  techs: TechDef[]
  lab_rates: LabRateInfo[]
  data_rates: Record<string, number>
  minutes_per_tick: number
}
```

- [ ] **Step 2: Add fetchContent to api.ts**

```typescript
export async function fetchContent(): Promise<ContentResponse> {
  const response = await fetch('/api/v1/content');
  if (!response.ok) { throw new Error(`Content fetch failed: ${response.status}`); }
  return response.json();
}
```

- [ ] **Step 3: Add test for fetchContent in api.test.ts**

```typescript
it('fetchContent returns content response', async () => {
  const mock = { techs: [], lab_rates: [], data_rates: {}, minutes_per_tick: 60 };
  vi.mocked(global.fetch).mockResolvedValueOnce(new Response(JSON.stringify(mock)));
  const result = await fetchContent();
  expect(result.techs).toEqual([]);
  expect(result.minutes_per_tick).toBe(60);
});
```

- [ ] **Step 4: Run tests**

Run: `npm test --prefix ui_web`
Expected: All tests pass.

- [ ] **Step 5: Commit**

Message: `VIO-293: add TechDef types and fetchContent API`

---

### Task 2: Create useContent Hook

**Files:**
- Create: `ui_web/src/hooks/useContent.ts`
- Create: `ui_web/src/hooks/useContent.test.ts`

- [ ] **Step 1: Write useContent.test.ts**

```typescript
import { renderHook, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import * as api from '../api';
import { useContent } from './useContent';

vi.mock('../api');

const mockContent = {
  techs: [{ id: 'tech_a', name: 'Tech A', prereqs: [], domain_requirements: {}, accepted_data: [], difficulty: 100, effects: [] }],
  lab_rates: [],
  data_rates: {},
  minutes_per_tick: 60,
};

describe('useContent', () => {
  it('fetches content on mount', async () => {
    vi.mocked(api.fetchContent).mockResolvedValue(mockContent);
    const { result } = renderHook(() => useContent());
    await waitFor(() => expect(result.current.content).not.toBeNull());
    expect(result.current.content?.techs).toHaveLength(1);
  });

  it('returns null before load', () => {
    vi.mocked(api.fetchContent).mockReturnValue(new Promise(() => {}));
    const { result } = renderHook(() => useContent());
    expect(result.current.content).toBeNull();
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test --prefix ui_web -- useContent`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement useContent.ts**

The hook refetches content periodically (every 30s) so rates stay current when labs become starved, get toggled, or modules are installed. The `tick` parameter triggers a refetch on significant state changes.

```typescript
import { useCallback, useEffect, useState } from 'react';
import type { ContentResponse } from '../types';
import { fetchContent } from '../api';

const REFETCH_INTERVAL_MS = 30_000;

export function useContent() {
  const [content, setContent] = useState<ContentResponse | null>(null);

  const refetch = useCallback(() => {
    fetchContent().then(setContent).catch(console.error);
  }, []);

  useEffect(() => {
    refetch();
    const interval = setInterval(refetch, REFETCH_INTERVAL_MS);
    return () => clearInterval(interval);
  }, [refetch]);

  return { content, refetch };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npm test --prefix ui_web -- useContent`
Expected: PASS

- [ ] **Step 5: Commit**

Message: `VIO-293: add useContent hook for content endpoint`

---

### Task 3: Build Tech Tree Visibility Logic (Pure Functions)

**Files:**
- Create: `ui_web/src/components/techTree.ts`
- Create: `ui_web/src/components/techTree.test.ts`

This is the core pure-logic module. No React, no rendering — just functions that compute visibility and node states from data.

- [ ] **Step 1: Write techTree.test.ts with visibility tests**

Test cases:
1. Unlocked techs are always visible with state 'unlocked'
2. Tech with lab assigned + all prereqs unlocked → 'researching'
3. Direct children of researching tech → 'locked' (one tier only)
4. Everything deeper → 'mystery'
5. Mystery nodes have same dimensions for layout
6. Edge states: active (to unlocked/researching), dim (to locked), fade (to mystery)
7. Empty state (no labs, no unlocked) → no visible nodes

```typescript
import { describe, expect, it } from 'vitest';
import type { TechDef, ResearchState } from '../types';
import { computeTreeState, type NodeState } from './techTree';

const makeTech = (id: string, prereqs: string[] = [], name?: string): TechDef => ({
  id, name: name ?? id, prereqs, domain_requirements: {}, accepted_data: [], difficulty: 100, effects: [],
});

describe('computeTreeState', () => {
  it('unlocked tech is visible with unlocked state', () => {
    const techs = [makeTech('a')];
    const research: ResearchState = { unlocked: ['a'], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, []);
    expect(result.nodes.get('a')?.state).toBe('unlocked');
  });

  it('tech with lab assigned and prereqs met is researching', () => {
    const techs = [makeTech('a')];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const labAssignments = ['a'];
    const result = computeTreeState(techs, research, labAssignments);
    expect(result.nodes.get('a')?.state).toBe('researching');
  });

  it('child of researching tech is locked (one tier)', () => {
    const techs = [makeTech('a'), makeTech('b', ['a'])];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, ['a']);
    expect(result.nodes.get('b')?.state).toBe('locked');
  });

  it('grandchild of researching tech is mystery', () => {
    const techs = [makeTech('a'), makeTech('b', ['a']), makeTech('c', ['b'])];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, ['a']);
    expect(result.nodes.get('c')?.state).toBe('mystery');
  });

  it('empty state returns no visible nodes', () => {
    const techs = [makeTech('a')];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, []);
    expect(result.nodes.size).toBe(0);
  });

  it('edge from researching to locked is dim', () => {
    const techs = [makeTech('a'), makeTech('b', ['a'])];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, ['a']);
    const edge = result.edges.find(e => e.from === 'a' && e.to === 'b');
    expect(edge?.style).toBe('dim');
  });

  it('locked child with mixed visible/invisible prereqs shows edges only from visible parents (spec rule 8)', () => {
    // a (researching), b (unknown/not visible), c depends on [a, b]
    const techs = [makeTech('a'), makeTech('b'), makeTech('c', ['a', 'b'])];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, ['a']);
    // c should be locked (child of researching 'a')
    expect(result.nodes.get('c')?.state).toBe('locked');
    // edge from a→c should exist
    expect(result.edges.find(e => e.from === 'a' && e.to === 'c')).toBeDefined();
    // edge from b→c should NOT exist (b is not visible)
    expect(result.edges.find(e => e.from === 'b' && e.to === 'c')).toBeUndefined();
  });

  it('converging edges into mystery zone (spec rule 6)', () => {
    // a (researching), b locked (child of a), c locked (child of a), d mystery (child of b+c)
    const techs = [makeTech('a'), makeTech('b', ['a']), makeTech('c', ['a']), makeTech('d', ['b', 'c'])];
    const research: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    const result = computeTreeState(techs, research, ['a']);
    expect(result.nodes.get('d')?.state).toBe('mystery');
    // Both edges b→d and c→d should exist (converging into mystery)
    expect(result.edges.find(e => e.from === 'b' && e.to === 'd')).toBeDefined();
    expect(result.edges.find(e => e.from === 'c' && e.to === 'd')).toBeDefined();
    // Both should be fade style
    expect(result.edges.find(e => e.to === 'd')?.style).toBe('fade');
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npm test --prefix ui_web -- techTree`
Expected: FAIL (module not found)

- [ ] **Step 3: Implement techTree.ts**

```typescript
import type { TechDef, ResearchState } from '../types';

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

export function computeTreeState(
  techs: TechDef[],
  research: ResearchState,
  labAssignments: string[],
): TreeState {
  const techMap = new Map(techs.map(t => [t.id, t]));
  const unlocked = new Set(research.unlocked);
  const assignedSet = new Set(labAssignments);

  // Build children lookup
  const children = new Map<string, string[]>();
  for (const tech of techs) {
    for (const prereq of tech.prereqs) {
      const existing = children.get(prereq) ?? [];
      existing.push(tech.id);
      children.set(prereq, existing);
    }
  }

  // Determine researching: lab assigned + all prereqs unlocked
  const researching = new Set<string>();
  for (const techId of assignedSet) {
    const tech = techMap.get(techId);
    if (tech && tech.prereqs.every(p => unlocked.has(p))) {
      researching.add(techId);
    }
  }

  // Determine locked: direct children of researching techs (not unlocked, not researching)
  const locked = new Set<string>();
  for (const rId of researching) {
    for (const childId of children.get(rId) ?? []) {
      if (!unlocked.has(childId) && !researching.has(childId)) {
        locked.add(childId);
      }
    }
  }

  // Determine mystery: children of locked nodes (not already categorized)
  const mystery = new Set<string>();
  for (const lId of locked) {
    for (const childId of children.get(lId) ?? []) {
      if (!unlocked.has(childId) && !researching.has(childId) && !locked.has(childId)) {
        mystery.add(childId);
      }
    }
  }

  // Build visible nodes
  const nodes = new Map<string, TreeNode>();
  const addNode = (techId: string, state: NodeVisibility) => {
    const tech = techMap.get(techId);
    if (!tech) return;
    const evidencePoints = research.evidence[techId]?.points ?? {};
    nodes.set(techId, {
      techId,
      name: state === 'mystery' ? '???' : tech.name,
      state,
      domainRequirements: tech.domain_requirements,
      evidence: evidencePoints,
    });
  };

  for (const id of unlocked) addNode(id, 'unlocked');
  for (const id of researching) addNode(id, 'researching');
  for (const id of locked) addNode(id, 'locked');
  for (const id of mystery) addNode(id, 'mystery');

  // Build edges between visible nodes only
  const edges: TreeEdge[] = [];
  for (const [techId, node] of nodes) {
    const tech = techMap.get(techId);
    if (!tech) continue;
    for (const prereqId of tech.prereqs) {
      if (nodes.has(prereqId)) {
        const fromState = nodes.get(prereqId)!.state;
        const toState = node.state;
        let style: EdgeStyle = 'active';
        if (toState === 'locked') style = 'dim';
        if (toState === 'mystery' || fromState === 'mystery') style = 'fade';
        edges.push({ from: prereqId, to: techId, style });
      }
    }
  }

  return { nodes, edges };
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npm test --prefix ui_web -- techTree`
Expected: PASS

- [ ] **Step 5: Commit**

Message: `VIO-293: add pure tech tree visibility logic`

---

### Task 4: Install dagre and Build TechTreeDAG Component

**Files:**
- Create: `ui_web/src/components/TechTreeDAG.tsx`
- Create: `ui_web/src/components/TechTreeDAG.test.tsx`
- Modify: `ui_web/package.json` (dagre dependency)

- [ ] **Step 1: Install dagre**

Run: `npm install --prefix ui_web dagre @types/dagre`

- [ ] **Step 2: Write TechTreeDAG.test.tsx**

```typescript
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { TechDef, ResearchState } from '../types';
import { TechTreeDAG } from './TechTreeDAG';

const techs: TechDef[] = [
  { id: 'a', name: 'Alpha Tech', prereqs: [], domain_requirements: { Survey: 100 }, accepted_data: [], difficulty: 200, effects: [] },
  { id: 'b', name: 'Beta Tech', prereqs: ['a'], domain_requirements: { Materials: 50 }, accepted_data: [], difficulty: 300, effects: [] },
];

const research: ResearchState = {
  unlocked: ['a'],
  data_pool: {},
  evidence: { b: { points: { Materials: 25 } } },
  action_counts: {},
};

describe('TechTreeDAG', () => {
  it('renders unlocked tech name', () => {
    render(<TechTreeDAG techs={techs} research={research} labAssignments={['b']} />);
    expect(screen.getByText('Alpha Tech')).toBeInTheDocument();
  });

  it('renders researching tech name', () => {
    render(<TechTreeDAG techs={techs} research={research} labAssignments={['b']} />);
    expect(screen.getByText('Beta Tech')).toBeInTheDocument();
  });

  it('renders empty state when no activity', () => {
    const emptyResearch: ResearchState = { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} };
    render(<TechTreeDAG techs={techs} research={emptyResearch} labAssignments={[]} />);
    expect(screen.getByText(/no research activity/i)).toBeInTheDocument();
  });

  it('renders mystery nodes as ???', () => {
    const deepTechs: TechDef[] = [
      ...techs,
      { id: 'c', name: 'Gamma', prereqs: ['b'], domain_requirements: {}, accepted_data: [], difficulty: 100, effects: [] },
    ];
    render(<TechTreeDAG techs={deepTechs} research={research} labAssignments={['b']} />);
    expect(screen.getByText('???')).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Implement TechTreeDAG.tsx**

Build the component using dagre for layout, HTML nodes with absolute positioning in a scrollable container, SVG overlay for edges. Uses `computeTreeState` from techTree.ts for visibility logic.

Key implementation details:
- `useMemo` to recompute tree state when research/techs change
- `useMemo` for dagre layout (only recompute when tree structure changes)
- Node width: 196px, height varies by domain count
- SVG overlay with `pointer-events: none`
- Edge colors: active `rgba(92,160,200,0.45)`, dim `#2a2e38`, fade `#1e2228` dashed
- Domain progress bars: 12px tall, fill at 35% opacity of domain color
- Numbers inside bar, right-aligned

- [ ] **Step 4: Run tests**

Run: `npm test --prefix ui_web -- TechTreeDAG`
Expected: PASS

- [ ] **Step 5: Run full FE test suite + lint**

Run: `npm test --prefix ui_web && npm run lint --prefix ui_web`
Expected: All pass, lint clean

- [ ] **Step 6: Commit**

Message: `VIO-293: add TechTreeDAG component with dagre layout`

---

### Task 5: Build DataPoolSection Component

**Files:**
- Create: `ui_web/src/components/DataPoolSection.tsx`
- Create: `ui_web/src/components/DataPoolSection.test.tsx`

- [ ] **Step 1: Write DataPoolSection.test.tsx**

```typescript
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { DataPoolSection } from './DataPoolSection';

describe('DataPoolSection', () => {
  it('renders data kind with amount and positive rate', () => {
    render(<DataPoolSection dataPool={{ SurveyData: 142.3 }} dataRates={{ SurveyData: 1.2 }} />);
    expect(screen.getByText(/Survey/)).toBeInTheDocument();
    expect(screen.getByText(/142\.3/)).toBeInTheDocument();
    expect(screen.getByText(/\+1\.2\/hr/)).toBeInTheDocument();
  });

  it('renders negative rate in red', () => {
    render(<DataPoolSection dataPool={{ AssayData: 87.1 }} dataRates={{ AssayData: -0.4 }} />);
    const rate = screen.getByText(/-0\.4\/hr/);
    expect(rate.className).toContain('text-red');
  });

  it('renders empty state', () => {
    render(<DataPoolSection dataPool={{}} dataRates={{}} />);
    expect(screen.getByText(/no data/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Implement DataPoolSection.tsx**

2-column grid. Color-coded by data kind. Rate shown inline: green for positive, red for negative. Strip "Data" suffix from kind names for display (SurveyData → Survey).

- [ ] **Step 3: Run tests**

Run: `npm test --prefix ui_web -- DataPoolSection`
Expected: PASS

- [ ] **Step 4: Commit**

Message: `VIO-294: add DataPoolSection component`

---

### Task 6: Build LabStatusSection Component

**Files:**
- Create: `ui_web/src/components/LabStatusSection.tsx`
- Create: `ui_web/src/components/LabStatusSection.test.tsx`

- [ ] **Step 1: Write LabStatusSection.test.tsx**

```typescript
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { LabRateInfo } from '../types';
import { LabStatusSection } from './LabStatusSection';

const labs: LabRateInfo[] = [
  { station_id: 's1', module_id: 'm1', module_name: 'Survey Lab', assigned_tech: 'tech_a', domain: 'Survey', points_per_hour: 3.2, starved: false, enabled: true },
  { station_id: 's1', module_id: 'm2', module_name: 'Materials Lab', assigned_tech: null, domain: 'Materials', points_per_hour: 0, starved: false, enabled: true },
];

describe('LabStatusSection', () => {
  it('renders lab name and rate', () => {
    render(<LabStatusSection labs={labs} techNames={{}} />);
    expect(screen.getByText('Survey Lab')).toBeInTheDocument();
    expect(screen.getByText(/\+3\.2\/hr/)).toBeInTheDocument();
  });

  it('shows idle badge for unassigned lab', () => {
    render(<LabStatusSection labs={labs} techNames={{}} />);
    expect(screen.getByText('idle')).toBeInTheDocument();
  });

  it('shows active badge for assigned non-starved lab', () => {
    render(<LabStatusSection labs={labs} techNames={{}} />);
    expect(screen.getByText('active')).toBeInTheDocument();
  });

  it('renders empty state', () => {
    render(<LabStatusSection labs={[]} techNames={{}} />);
    expect(screen.getByText(/no labs/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Implement LabStatusSection.tsx**

Compact rows: lab name, assigned tech name, production rate, status badge.
Status badges: active (green), starved (red), idle (gray).

- [ ] **Step 3: Run tests**

Run: `npm test --prefix ui_web -- LabStatusSection`
Expected: PASS

- [ ] **Step 4: Commit**

Message: `VIO-294: add LabStatusSection component`

---

### Task 7: Redesign ResearchPanel

**Files:**
- Modify: `ui_web/src/components/ResearchPanel.tsx`
- Modify: `ui_web/src/components/ResearchPanel.test.tsx`

- [ ] **Step 1: Update ResearchPanel.test.tsx**

Replace the existing tests with tests for the new 3-section layout:

```typescript
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { ResearchState, ContentResponse } from '../types';
import { ResearchPanel } from './ResearchPanel';

vi.mock('../hooks/useContent', () => ({
  useContent: () => ({
    content: {
      techs: [{ id: 'tech_a', name: 'Alpha', prereqs: [], domain_requirements: { Survey: 100 }, accepted_data: [], difficulty: 200, effects: [] }],
      lab_rates: [{ station_id: 's1', module_id: 'm1', module_name: 'Survey Lab', assigned_tech: 'tech_a', domain: 'Survey', points_per_hour: 4.0, starved: false, enabled: true }],
      data_rates: { SurveyData: 1.2 },
      minutes_per_tick: 60,
    },
  }),
}));

const research: ResearchState = {
  unlocked: [],
  data_pool: { SurveyData: 42.5 },
  evidence: { tech_a: { points: { Survey: 50 } } },
  action_counts: {},
};

describe('ResearchPanel', () => {
  it('renders data pool section', () => {
    render(<ResearchPanel research={research} />);
    expect(screen.getByText(/Survey/)).toBeInTheDocument();
    expect(screen.getByText(/42\.5/)).toBeInTheDocument();
  });

  it('renders lab status section', () => {
    render(<ResearchPanel research={research} />);
    expect(screen.getByText('Survey Lab')).toBeInTheDocument();
  });

  it('renders tech tree', () => {
    render(<ResearchPanel research={research} />);
    expect(screen.getByText('Alpha')).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Rewrite ResearchPanel.tsx**

Three sections top to bottom:
1. `<DataPoolSection>` — data pool amounts + rates
2. `<TechTreeDAG>` — DAG visualization (flex-1 to fill remaining space)
3. `<LabStatusSection>` — lab rows

Uses `useContent()` hook. Extracts lab assignments from `content.lab_rates` for the DAG. Builds `techNames` map for LabStatusSection.

- [ ] **Step 3: Run tests**

Run: `npm test --prefix ui_web -- ResearchPanel`
Expected: PASS

- [ ] **Step 4: Run full FE test suite + lint + typecheck**

Run: `npm test --prefix ui_web && npm run lint --prefix ui_web`
Expected: All pass

- [ ] **Step 5: Run event sync check**

Run: `./scripts/ci_event_sync.sh`
Expected: OK

- [ ] **Step 6: Commit**

Message: `VIO-294: redesign ResearchPanel with data pool, tech tree, and lab status`

---

### Task 8: Visual Verification and Polish

- [ ] **Step 1: Start daemon and UI**

Run: `cargo run -p sim_daemon -- run --seed 42` (background)
Run: `npm run dev --prefix ui_web` (background)

- [ ] **Step 2: Verify in browser**

Open `localhost:5173`, navigate to Research panel. Verify:
- Data pool shows data kinds with color coding and rates
- Tech tree renders DAG with correct node states
- Lab status shows labs with rates and status badges
- Scrolling works for large trees
- Live updates via SSE

- [ ] **Step 3: Run full CI suite locally**

Run: `./scripts/ci_rust.sh && ./scripts/ci_web.sh`
Expected: All pass

- [ ] **Step 4: Commit any polish fixes**
