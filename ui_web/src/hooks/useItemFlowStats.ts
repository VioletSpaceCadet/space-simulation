import { useReducer } from 'react';

import type { ComponentItem, ItemFlowStats, MaterialItem, SimSnapshot } from '../types';

function buildInventoryTotals(snapshot: SimSnapshot): Map<string, number> {
  const totals = new Map<string, number>();
  for (const station of Object.values(snapshot.stations)) {
    for (const item of station.inventory) {
      switch (item.kind) {
        case 'Ore':
          totals.set('ore', (totals.get('ore') ?? 0) + item.kg);
          break;
        case 'Slag':
          totals.set('slag', (totals.get('slag') ?? 0) + item.kg);
          break;
        case 'Material':
          totals.set(
            (item as MaterialItem).element,
            (totals.get((item as MaterialItem).element) ?? 0) + item.kg,
          );
          break;
        case 'Component':
          totals.set(
            (item as ComponentItem).component_id,
            (totals.get((item as ComponentItem).component_id) ?? 0) + (item as ComponentItem).count,
          );
          break;
      }
    }
  }
  return totals;
}

interface FlowState {
  stats: Map<string, ItemFlowStats>
  prevInventory: Map<string, number>
  ticksAtZero: Map<string, number>
  lastSnapshot: SimSnapshot | null
  lastMinutesPerTick: number
}

interface FlowAction {
  snapshot: SimSnapshot | null
  minutesPerTick: number
}

function flowReducer(state: FlowState, action: FlowAction): FlowState {
  const { snapshot, minutesPerTick } = action;

  // Same inputs — return previous state
  if (snapshot === state.lastSnapshot && minutesPerTick === state.lastMinutesPerTick) {
    return state;
  }

  if (!snapshot) {
    return {
      stats: new Map(),
      prevInventory: state.prevInventory,
      ticksAtZero: state.ticksAtZero,
      lastSnapshot: null,
      lastMinutesPerTick: minutesPerTick,
    };
  }

  const currentInventory = buildInventoryTotals(snapshot);
  const hoursPerTick = minutesPerTick / 60;
  const result = new Map<string, ItemFlowStats>();
  const newTicksAtZero = new Map(state.ticksAtZero);

  for (const [itemId, qty] of currentInventory) {
    const prevQty = state.prevInventory.get(itemId) ?? qty;
    const delta = qty - prevQty;
    const deltaPerHour = hoursPerTick > 0 ? delta / hoursPerTick : 0;

    const threshold = Math.max(Math.abs(prevQty) * 0.01, 0.1);
    const trend: 'rising' | 'falling' | 'stable' =
      delta > threshold ? 'rising' : delta < -threshold ? 'falling' : 'stable';

    const zeroCount = state.ticksAtZero.get(itemId) ?? 0;
    const newZeroCount = qty < 0.01 ? zeroCount + 1 : 0;
    newTicksAtZero.set(itemId, newZeroCount);

    result.set(itemId, {
      item_id: itemId,
      current_qty: qty,
      delta_per_hour: deltaPerHour,
      trend,
      ticks_at_zero: newZeroCount,
    });
  }

  return {
    stats: result,
    prevInventory: currentInventory,
    ticksAtZero: newTicksAtZero,
    lastSnapshot: snapshot,
    lastMinutesPerTick: minutesPerTick,
  };
}

const INITIAL_STATE: FlowState = {
  stats: new Map(),
  prevInventory: new Map(),
  ticksAtZero: new Map(),
  lastSnapshot: null,
  lastMinutesPerTick: 0,
};

/**
 * Computes per-item flow statistics (delta, trend, starvation counter) by
 * comparing successive inventory snapshots.
 *
 * Implemented with useReducer to track previous inventory state without
 * reading refs during render.
 */
export function useItemFlowStats(
  snapshot: SimSnapshot | null,
  minutesPerTick: number,
): Map<string, ItemFlowStats> {
  const [state, dispatch] = useReducer(flowReducer, INITIAL_STATE);

  // Dispatch on every render with new inputs; the reducer short-circuits if
  // snapshot + minutesPerTick haven't changed.
  const action: FlowAction = { snapshot, minutesPerTick };
  if (snapshot !== state.lastSnapshot || minutesPerTick !== state.lastMinutesPerTick) {
    dispatch(action);
  }

  return state.stats;
}
