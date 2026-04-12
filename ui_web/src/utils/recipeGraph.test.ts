import { describe, expect, it } from 'vitest';

import type { RecipeDef, RecipeInput, StationState } from '../types';

import {
  buildRecipeGraph,
  parseRecipeInputAmount,
  parseRecipeInputItem,
  parseRecipeOutput,
} from './recipeGraph';

function makeStation(overrides: Partial<StationState> = {}): StationState {
  return {
    id: 'station_001',
    position: { parent_body: 'body_a', radius_au_um: 0, angle_mdeg: 0 },
    power_available_per_tick: 10,
    inventory: [],
    cargo_capacity_m3: 100,
    modules: [],
    power: {
      generated_kw: 0, consumed_kw: 0, deficit_kw: 0,
      battery_discharge_kw: 0, battery_charge_kw: 0, battery_stored_kwh: 0,
    },
    ...overrides,
  };
}

describe('parseRecipeInputItem', () => {
  it('parses ItemKind Ore', () => {
    const input: RecipeInput = { filter: { ItemKind: 'Ore' }, amount: { Kg: 500 } };
    expect(parseRecipeInputItem(input)).toEqual({ itemId: 'ore', type: 'raw' });
  });

  it('parses Element filter', () => {
    const input: RecipeInput = { filter: { Element: 'Fe' }, amount: { Kg: 200 } };
    expect(parseRecipeInputItem(input)).toEqual({ itemId: 'Fe', type: 'refined' });
  });

  it('parses Component filter', () => {
    const input: RecipeInput = { filter: { Component: 'fe_plate' }, amount: { Count: 3 } };
    expect(parseRecipeInputItem(input)).toEqual({ itemId: 'fe_plate', type: 'component' });
  });

  it('parses Module filter', () => {
    const input: RecipeInput = { filter: { Module: 'module_cargo_expander' }, amount: { Count: 1 } };
    expect(parseRecipeInputItem(input)).toEqual({ itemId: 'module_cargo_expander', type: 'component' });
  });
});

describe('parseRecipeInputAmount', () => {
  it('parses Kg amount', () => {
    const input: RecipeInput = { filter: { ItemKind: 'Ore' }, amount: { Kg: 500 } };
    expect(parseRecipeInputAmount(input)).toEqual({ amount: 500, unit: 'kg' });
  });

  it('parses Count amount', () => {
    const input: RecipeInput = { filter: { Component: 'fe_plate' }, amount: { Count: 3 } };
    expect(parseRecipeInputAmount(input)).toEqual({ amount: 3, unit: 'count' });
  });
});

describe('parseRecipeOutput', () => {
  it('parses Material output', () => {
    const output = { Material: { element: 'Fe', yield_formula: {}, quality_formula: {} } };
    expect(parseRecipeOutput(output)).toEqual({ itemId: 'Fe', type: 'refined', unit: 'kg' });
  });

  it('parses Slag output', () => {
    const output = { Slag: { yield_formula: {} } };
    expect(parseRecipeOutput(output)).toEqual({ itemId: 'slag', type: 'raw', unit: 'kg' });
  });

  it('parses Component output', () => {
    const output = { Component: { component_id: 'fe_plate', quality_formula: {} } };
    expect(parseRecipeOutput(output)).toEqual({ itemId: 'fe_plate', type: 'component', unit: 'count' });
  });

  it('parses Ship output', () => {
    const output = { Ship: { cargo_capacity_m3: 50 } };
    expect(parseRecipeOutput(output)).toEqual({ itemId: 'ship', type: 'ship', unit: 'count' });
  });
});

describe('buildRecipeGraph', () => {
  it('returns empty graph for empty recipes', () => {
    const graph = buildRecipeGraph({}, {}, []);
    expect(graph.recipeNodes.size).toBe(0);
    expect(graph.itemNodes.size).toBe(0);
    expect(graph.edges).toHaveLength(0);
  });

  it('builds single recipe with correct nodes and edges', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_fe_plate: {
        id: 'recipe_fe_plate',
        inputs: [{ filter: { Element: 'Fe' }, amount: { Kg: 500 } }],
        outputs: [{ Component: { component_id: 'fe_plate', quality_formula: {} } }],
        efficiency: 1.0,
      },
    };
    const graph = buildRecipeGraph(recipes, {}, []);

    expect(graph.recipeNodes.size).toBe(1);
    expect(graph.itemNodes.size).toBe(2); // Fe + fe_plate
    expect(graph.edges).toHaveLength(2); // Fe -> recipe, recipe -> fe_plate

    const feNode = graph.itemNodes.get('Fe');
    expect(feNode).toBeDefined();
    expect(feNode!.type).toBe('refined');

    const plateNode = graph.itemNodes.get('fe_plate');
    expect(plateNode).toBeDefined();
    expect(plateNode!.type).toBe('component');
  });

  it('builds multi-tier chain with correct edges', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_fe_plate: {
        id: 'recipe_fe_plate',
        inputs: [{ filter: { Element: 'Fe' }, amount: { Kg: 500 } }],
        outputs: [{ Component: { component_id: 'fe_plate', quality_formula: {} } }],
        efficiency: 1.0,
      },
      recipe_structural_beam: {
        id: 'recipe_structural_beam',
        inputs: [{ filter: { Component: 'fe_plate' }, amount: { Count: 3 } }],
        outputs: [{ Component: { component_id: 'structural_beam', quality_formula: {} } }],
        efficiency: 1.0,
      },
    };
    const graph = buildRecipeGraph(recipes, {}, []);

    expect(graph.recipeNodes.size).toBe(2);
    // Fe, fe_plate, structural_beam
    expect(graph.itemNodes.size).toBe(3);
    // Fe->recipe_fe_plate, recipe_fe_plate->fe_plate, fe_plate->recipe_structural_beam, recipe_structural_beam->structural_beam
    expect(graph.edges).toHaveLength(4);
  });

  it('excludes locked recipes when required_tech is not unlocked', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_hull_panel: {
        id: 'recipe_hull_panel',
        inputs: [{ filter: { Component: 'structural_beam' }, amount: { Count: 2 } }],
        outputs: [{ Component: { component_id: 'hull_panel', quality_formula: {} } }],
        efficiency: 1.0,
        required_tech: 'tech_advanced_manufacturing',
      },
    };
    const graph = buildRecipeGraph(recipes, {}, []);
    expect(graph.recipeNodes.size).toBe(0);
    expect(graph.itemNodes.size).toBe(0);
    expect(graph.edges).toHaveLength(0);
  });

  it('includes recipe when required_tech is unlocked', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_hull_panel: {
        id: 'recipe_hull_panel',
        inputs: [{ filter: { Component: 'structural_beam' }, amount: { Count: 2 } }],
        outputs: [{ Component: { component_id: 'hull_panel', quality_formula: {} } }],
        efficiency: 1.0,
        required_tech: 'tech_advanced_manufacturing',
      },
    };
    const graph = buildRecipeGraph(recipes, {}, ['tech_advanced_manufacturing']);
    expect(graph.recipeNodes.size).toBe(1);
    expect(graph.recipeNodes.get('recipe_hull_panel')!.status).toBe('available');
  });

  it('creates fan-in edges for recipe with multiple inputs', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_hull_panel: {
        id: 'recipe_hull_panel',
        inputs: [
          { filter: { Component: 'structural_beam' }, amount: { Count: 2 } },
          { filter: { Component: 'fe_plate' }, amount: { Count: 2 } },
        ],
        outputs: [{ Component: { component_id: 'hull_panel', quality_formula: {} } }],
        efficiency: 1.0,
      },
    };
    const graph = buildRecipeGraph(recipes, {}, []);

    // 2 input edges + 1 output edge
    expect(graph.edges).toHaveLength(3);
    const inputEdges = graph.edges.filter((e) => e.to === 'recipe:recipe_hull_panel');
    expect(inputEdges).toHaveLength(2);
  });

  it('looks up inventory quantities from station state', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_fe_plate: {
        id: 'recipe_fe_plate',
        inputs: [{ filter: { Element: 'Fe' }, amount: { Kg: 500 } }],
        outputs: [{ Component: { component_id: 'fe_plate', quality_formula: {} } }],
        efficiency: 1.0,
      },
    };
    const stations: Record<string, StationState> = {
      station_001: makeStation({
        inventory: [
          { kind: 'Material', element: 'Fe', kg: 1200, quality: 1.0 },
          { kind: 'Component', component_id: 'fe_plate', count: 5, quality: 1.0 },
        ],
      }),
    };
    const graph = buildRecipeGraph(recipes, stations, []);

    expect(graph.itemNodes.get('Fe')!.inventory).toBe(1200);
    expect(graph.itemNodes.get('fe_plate')!.inventory).toBe(5);
  });

  it('marks recipe as active when a station module has it selected', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_fe_plate: {
        id: 'recipe_fe_plate',
        inputs: [{ filter: { Element: 'Fe' }, amount: { Kg: 500 } }],
        outputs: [{ Component: { component_id: 'fe_plate', quality_formula: {} } }],
        efficiency: 1.0,
      },
    };
    const stations: Record<string, StationState> = {
      station_001: makeStation({
        modules: [
          {
            id: 'mod_assembler_001',
            def_id: 'assembler_basic',
            enabled: true,
            kind_state: {
              Assembler: {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: {},
                selected_recipe: 'recipe_fe_plate',
              },
            },
            wear: { wear: 0 },
          },
        ],
      }),
    };
    const graph = buildRecipeGraph(recipes, stations, []);
    expect(graph.recipeNodes.get('recipe_fe_plate')!.status).toBe('active');
  });

  it('marks recipe as available when module is disabled', () => {
    const recipes: Record<string, RecipeDef> = {
      recipe_fe_plate: {
        id: 'recipe_fe_plate',
        inputs: [{ filter: { Element: 'Fe' }, amount: { Kg: 500 } }],
        outputs: [{ Component: { component_id: 'fe_plate', quality_formula: {} } }],
        efficiency: 1.0,
      },
    };
    const stations: Record<string, StationState> = {
      station_001: makeStation({
        modules: [
          {
            id: 'mod_assembler_001',
            def_id: 'assembler_basic',
            enabled: false,
            kind_state: {
              Assembler: {
                ticks_since_last_run: 0,
                stalled: false,
                capped: false,
                cap_override: {},
                selected_recipe: 'recipe_fe_plate',
              },
            },
            wear: { wear: 0 },
          },
        ],
      }),
    };
    const graph = buildRecipeGraph(recipes, stations, []);
    expect(graph.recipeNodes.get('recipe_fe_plate')!.status).toBe('available');
  });
});
