import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { SimEvent } from '../types';

import { useModuleFlowStats } from './useModuleFlowStats';

function makeRefineryEvent(tick: number, moduleId: string, oreKg: number, materialKg: number): SimEvent {
  return {
    id: tick,
    tick,
    event: {
      RefineryRan: {
        station_id: 'station_001',
        module_id: moduleId,
        ore_consumed_kg: oreKg,
        material_produced_kg: materialKg,
      },
    },
  };
}

function makeAssemblerEvent(
  tick: number, moduleId: string, recipeId: string,
  materialKg: number, componentCount: number,
): SimEvent {
  return {
    id: tick,
    tick,
    event: {
      AssemblerRan: {
        station_id: 'station_001',
        module_id: moduleId,
        recipe_id: recipeId,
        material_consumed_kg: materialKg,
        component_produced_count: componentCount,
      },
    },
  };
}

describe('useModuleFlowStats', () => {
  it('returns empty map when no events', () => {
    const { result } = renderHook(() => useModuleFlowStats([], 100, 60));
    expect(result.current.size).toBe(0);
  });

  it('aggregates RefineryRan events correctly', () => {
    const events = [
      makeRefineryEvent(90, 'mod_refinery_001', 1000, 300),
      makeRefineryEvent(95, 'mod_refinery_001', 800, 250),
    ];
    const { result } = renderHook(() => useModuleFlowStats(events, 100, 60, 100));
    const stat = result.current.get('mod_refinery_001');

    expect(stat).toBeDefined();
    expect(stat!.runs_in_window).toBe(2);
    expect(stat!.total_input_kg).toBe(1800);
    expect(stat!.total_output_kg).toBe(550);
    expect(stat!.last_run_tick).toBe(95);
  });

  it('aggregates AssemblerRan events correctly', () => {
    const events = [
      makeAssemblerEvent(80, 'mod_assembler_001', 'recipe_fe_plate', 500, 1),
      makeAssemblerEvent(90, 'mod_assembler_001', 'recipe_fe_plate', 500, 1),
    ];
    const { result } = renderHook(() => useModuleFlowStats(events, 100, 60, 100));
    const stat = result.current.get('mod_assembler_001');

    expect(stat).toBeDefined();
    expect(stat!.runs_in_window).toBe(2);
    expect(stat!.recipe_id).toBe('recipe_fe_plate');
    expect(stat!.total_input_kg).toBe(1000);
    expect(stat!.total_output_count).toBe(2);
  });

  it('excludes events outside the window', () => {
    const events = [
      makeRefineryEvent(10, 'mod_refinery_001', 1000, 300),   // outside window (100 - 50 = 50)
      makeRefineryEvent(60, 'mod_refinery_001', 500, 150),    // inside window
    ];
    const { result } = renderHook(() => useModuleFlowStats(events, 100, 60, 50));
    const stat = result.current.get('mod_refinery_001');

    expect(stat).toBeDefined();
    expect(stat!.runs_in_window).toBe(1);
    expect(stat!.total_input_kg).toBe(500);
  });

  it('computes throughput per hour correctly', () => {
    // windowSize=100 ticks, minutesPerTick=60 => 100 hours
    const events = [
      makeRefineryEvent(90, 'mod_refinery_001', 1000, 500),
    ];
    const { result } = renderHook(() => useModuleFlowStats(events, 100, 60, 100));
    const stat = result.current.get('mod_refinery_001');

    // 500 kg / 100 hours = 5 kg/hr
    expect(stat!.throughput_per_hour).toBeCloseTo(5);
  });

  it('computes utilization percentage correctly', () => {
    const events = [
      makeRefineryEvent(90, 'mod_refinery_001', 1000, 300),
      makeRefineryEvent(95, 'mod_refinery_001', 800, 250),
    ];
    const { result } = renderHook(() => useModuleFlowStats(events, 100, 60, 100));
    const stat = result.current.get('mod_refinery_001');

    // 2 runs / 100 ticks * 100 = 2%
    expect(stat!.utilization_pct).toBeCloseTo(2);
  });
});
