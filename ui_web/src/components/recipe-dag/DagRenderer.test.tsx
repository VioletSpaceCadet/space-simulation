import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { RecipeGraph } from '../../utils/recipeGraph';

import { DagRenderer } from './DagRenderer';

function emptyGraph(): RecipeGraph {
  return {
    recipeNodes: new Map(),
    itemNodes: new Map(),
    edges: [],
  };
}

function sampleGraph(): RecipeGraph {
  return {
    recipeNodes: new Map([
      ['smelt_fe', {
        id: 'smelt_fe',
        inputs: [{ itemId: 'ore', amount: 10, unit: 'kg' as const }],
        outputs: [{ itemId: 'Fe', amount: 0, unit: 'kg' as const }],
        status: 'active' as const,
      }],
      ['assemble_hull', {
        id: 'assemble_hull',
        inputs: [{ itemId: 'Fe', amount: 5, unit: 'kg' as const }],
        outputs: [{ itemId: 'hull_plate', amount: 1, unit: 'count' as const }],
        status: 'available' as const,
      }],
    ]),
    itemNodes: new Map([
      ['ore', { id: 'ore', type: 'raw' as const, name: 'ore', inventory: 100 }],
      ['Fe', { id: 'Fe', type: 'refined' as const, name: 'Fe', inventory: 50 }],
      ['hull_plate', { id: 'hull_plate', type: 'component' as const, name: 'hull_plate', inventory: 3 }],
    ]),
    edges: [
      { from: 'item:ore', to: 'recipe:smelt_fe', recipeId: 'smelt_fe' },
      { from: 'recipe:smelt_fe', to: 'item:Fe', recipeId: 'smelt_fe' },
      { from: 'item:Fe', to: 'recipe:assemble_hull', recipeId: 'assemble_hull' },
      { from: 'recipe:assemble_hull', to: 'item:hull_plate', recipeId: 'assemble_hull' },
    ],
  };
}

const noop = vi.fn();

describe('DagRenderer', () => {
  it('renders empty state for empty graph', () => {
    render(
      <DagRenderer
        graph={emptyGraph()}
        moduleFlowStats={new Map()}
        itemFlowStats={new Map()}
        selectedNodeId={null}
        onNodeSelect={noop}
        onNodeHover={noop}
        filter="all"
      />,
    );
    expect(screen.getByText(/no recipes available/i)).toBeInTheDocument();
  });

  it('renders correct number of item and recipe nodes', () => {
    render(
      <DagRenderer
        graph={sampleGraph()}
        moduleFlowStats={new Map()}
        itemFlowStats={new Map()}
        selectedNodeId={null}
        onNodeSelect={noop}
        onNodeHover={noop}
        filter="all"
      />,
    );
    expect(screen.getByTestId('item-node-ore')).toBeInTheDocument();
    expect(screen.getByTestId('item-node-Fe')).toBeInTheDocument();
    expect(screen.getByTestId('item-node-hull_plate')).toBeInTheDocument();
    expect(screen.getByTestId('recipe-node-smelt_fe')).toBeInTheDocument();
    expect(screen.getByTestId('recipe-node-assemble_hull')).toBeInTheDocument();
  });

  it('recipe node shows status indicator', () => {
    render(
      <DagRenderer
        graph={sampleGraph()}
        moduleFlowStats={new Map()}
        itemFlowStats={new Map()}
        selectedNodeId={null}
        onNodeSelect={noop}
        onNodeHover={noop}
        filter="all"
      />,
    );
    const activeIndicator = screen.getByTestId('recipe-status-smelt_fe');
    expect(activeIndicator).toBeInTheDocument();
    // Active recipe should have the active color
    expect(activeIndicator.style.background).toBe('rgb(76, 175, 125)');
  });

  it('filters to only active recipes when filter is active', () => {
    render(
      <DagRenderer
        graph={sampleGraph()}
        moduleFlowStats={new Map()}
        itemFlowStats={new Map()}
        selectedNodeId={null}
        onNodeSelect={noop}
        onNodeHover={noop}
        filter="active"
      />,
    );
    // Active recipe should still be visible
    expect(screen.getByTestId('recipe-node-smelt_fe')).toBeInTheDocument();
    // Available recipe should NOT be visible
    expect(screen.queryByTestId('recipe-node-assemble_hull')).not.toBeInTheDocument();
  });
});
