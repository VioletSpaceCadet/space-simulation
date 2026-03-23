import type { ComponentItem, MaterialItem, RecipeDef, RecipeInput, StationState } from '../types';

export interface RecipeNode {
  id: string
  inputs: { itemId: string; amount: number; unit: 'kg' | 'count' }[]
  outputs: { itemId: string; amount: number; unit: 'kg' | 'count' }[]
  status: 'active' | 'available'
}

export interface ItemNode {
  id: string
  type: 'raw' | 'refined' | 'component' | 'ship'
  name: string
  inventory: number
}

export interface GraphEdge {
  from: string
  to: string
  recipeId: string
}

export interface RecipeGraph {
  recipeNodes: Map<string, RecipeNode>
  itemNodes: Map<string, ItemNode>
  edges: GraphEdge[]
}

interface ParsedItem {
  itemId: string
  type: 'raw' | 'refined' | 'component' | 'ship'
}

interface ParsedAmount {
  amount: number
  unit: 'kg' | 'count'
}

export function parseRecipeInputItem(input: RecipeInput): ParsedItem {
  const filter = input.filter;
  if ('ItemKind' in filter) {
    const kind = filter.ItemKind as string;
    if (kind === 'Ore') { return { itemId: 'ore', type: 'raw' }; }
    if (kind === 'Slag') { return { itemId: 'slag', type: 'raw' }; }
    if (kind === 'Material') { return { itemId: 'material', type: 'refined' }; }
    if (kind === 'Component') { return { itemId: 'component', type: 'component' }; }
    return { itemId: kind.toLowerCase(), type: 'raw' };
  }
  if ('Element' in filter) {
    return { itemId: filter.Element as string, type: 'refined' };
  }
  if ('ElementWithMinQuality' in filter) {
    const nested = filter.ElementWithMinQuality as unknown as { element: string };
    return { itemId: nested.element, type: 'refined' };
  }
  if ('Component' in filter) {
    return { itemId: filter.Component as string, type: 'component' };
  }
  return { itemId: 'unknown', type: 'raw' };
}

export function parseRecipeInputAmount(input: RecipeInput): ParsedAmount {
  const amount = input.amount;
  if ('Kg' in amount) {
    return { amount: amount.Kg, unit: 'kg' };
  }
  if ('Count' in amount) {
    return { amount: amount.Count, unit: 'count' };
  }
  return { amount: 0, unit: 'kg' };
}

interface ParsedOutput {
  itemId: string
  type: 'raw' | 'refined' | 'component' | 'ship'
  unit: 'kg' | 'count'
}

export function parseRecipeOutput(output: Record<string, unknown>): ParsedOutput {
  if ('Material' in output) {
    const material = output.Material as { element: string };
    return { itemId: material.element, type: 'refined', unit: 'kg' };
  }
  if ('Slag' in output) {
    return { itemId: 'slag', type: 'raw', unit: 'kg' };
  }
  if ('Component' in output) {
    const component = output.Component as { component_id: string };
    return { itemId: component.component_id, type: 'component', unit: 'count' };
  }
  if ('Ship' in output) {
    return { itemId: 'ship', type: 'ship', unit: 'count' };
  }
  return { itemId: 'unknown', type: 'raw', unit: 'kg' };
}

function collectActiveRecipeIds(stations: Record<string, StationState>): Set<string> {
  const activeRecipeIds = new Set<string>();
  for (const station of Object.values(stations)) {
    for (const module of station.modules) {
      if (!module.enabled) { continue; }
      const kindState = module.kind_state;
      if (typeof kindState === 'object') {
        if ('Processor' in kindState) {
          const recipe = kindState.Processor.selected_recipe;
          if (recipe) { activeRecipeIds.add(recipe); }
        } else if ('Assembler' in kindState) {
          const recipe = kindState.Assembler.selected_recipe;
          if (recipe) { activeRecipeIds.add(recipe); }
        }
      }
    }
  }
  return activeRecipeIds;
}

function collectInventoryTotals(stations: Record<string, StationState>): Map<string, number> {
  const inventory = new Map<string, number>();
  for (const station of Object.values(stations)) {
    for (const item of station.inventory) {
      switch (item.kind) {
        case 'Ore':
          inventory.set('ore', (inventory.get('ore') ?? 0) + item.kg);
          break;
        case 'Slag':
          inventory.set('slag', (inventory.get('slag') ?? 0) + item.kg);
          break;
        case 'Material':
          inventory.set(
            (item as MaterialItem).element,
            (inventory.get((item as MaterialItem).element) ?? 0) + item.kg,
          );
          break;
        case 'Component':
          inventory.set(
            (item as ComponentItem).component_id,
            (inventory.get((item as ComponentItem).component_id) ?? 0) + (item as ComponentItem).count,
          );
          break;
      }
    }
  }
  return inventory;
}

export function buildRecipeGraph(
  recipes: Record<string, RecipeDef>,
  stations: Record<string, StationState>,
  unlockedTechs: string[],
): RecipeGraph {
  const recipeNodes = new Map<string, RecipeNode>();
  const itemNodes = new Map<string, ItemNode>();
  const edges: GraphEdge[] = [];

  const unlockedSet = new Set(unlockedTechs);
  const activeRecipeIds = collectActiveRecipeIds(stations);
  const inventoryTotals = collectInventoryTotals(stations);

  for (const recipe of Object.values(recipes)) {
    // Skip locked recipes
    if (recipe.required_tech && !unlockedSet.has(recipe.required_tech)) {
      continue;
    }

    const status: RecipeNode['status'] = activeRecipeIds.has(recipe.id) ? 'active' : 'available';

    const inputs: RecipeNode['inputs'] = recipe.inputs.map((input) => {
      const parsed = parseRecipeInputItem(input);
      const parsedAmount = parseRecipeInputAmount(input);
      return { itemId: parsed.itemId, amount: parsedAmount.amount, unit: parsedAmount.unit };
    });

    const outputs: RecipeNode['outputs'] = recipe.outputs.map((output) => {
      const parsed = parseRecipeOutput(output);
      // Ship outputs use the recipe name as item ID for a meaningful display name
      if (parsed.type === 'ship') {
        parsed.itemId = recipe.id.replace(/^recipe_/, '');
      }
      return { itemId: parsed.itemId, amount: 0, unit: parsed.unit };
    });

    recipeNodes.set(recipe.id, { id: recipe.id, inputs, outputs, status });

    // Create item nodes and edges for inputs
    for (const input of recipe.inputs) {
      const parsed = parseRecipeInputItem(input);
      if (!itemNodes.has(parsed.itemId)) {
        itemNodes.set(parsed.itemId, {
          id: parsed.itemId,
          type: parsed.type,
          name: parsed.itemId,
          inventory: inventoryTotals.get(parsed.itemId) ?? 0,
        });
      }
      edges.push({ from: `item:${parsed.itemId}`, to: `recipe:${recipe.id}`, recipeId: recipe.id });
    }

    // Create item nodes and edges for outputs
    for (const output of recipe.outputs) {
      const parsed = parseRecipeOutput(output);
      // Ship outputs use recipe-derived name for meaningful display
      if (parsed.type === 'ship') {
        parsed.itemId = recipe.id.replace(/^recipe_/, '');
      }
      if (!itemNodes.has(parsed.itemId)) {
        itemNodes.set(parsed.itemId, {
          id: parsed.itemId,
          type: parsed.type,
          name: parsed.itemId,
          inventory: inventoryTotals.get(parsed.itemId) ?? 0,
        });
      }
      edges.push({ from: `recipe:${recipe.id}`, to: `item:${parsed.itemId}`, recipeId: recipe.id });
    }
  }

  return { recipeNodes, itemNodes, edges };
}
