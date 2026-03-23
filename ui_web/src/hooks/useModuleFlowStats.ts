import { useMemo } from 'react';

import type { ModuleFlowStats, SimEvent } from '../types';

export function useModuleFlowStats(
  events: SimEvent[],
  currentTick: number,
  minutesPerTick: number,
  windowSize = 100,
): Map<string, ModuleFlowStats> {
  return useMemo(() => {
    const stats = new Map<string, ModuleFlowStats>();
    const windowStart = currentTick - windowSize;

    for (const evt of events) {
      if (evt.tick < windowStart) { continue; }

      const refineryRan = (evt.event as Record<string, unknown>).RefineryRan as {
        station_id: string; module_id: string;
        ore_consumed_kg: number; material_produced_kg: number;
      } | undefined;

      if (refineryRan) {
        const key = refineryRan.module_id;
        const existing = stats.get(key) ?? {
          module_id: key, recipe_id: '', runs_in_window: 0,
          total_input_kg: 0, total_output_kg: 0, total_output_count: 0,
          last_run_tick: 0, throughput_per_hour: 0, utilization_pct: 0,
          stall_reason: null,
        };
        existing.runs_in_window++;
        existing.total_input_kg += refineryRan.ore_consumed_kg;
        existing.total_output_kg += refineryRan.material_produced_kg;
        existing.last_run_tick = Math.max(existing.last_run_tick, evt.tick);
        stats.set(key, existing);
      }

      const assemblerRan = (evt.event as Record<string, unknown>).AssemblerRan as {
        station_id: string; module_id: string; recipe_id: string;
        material_consumed_kg: number; component_produced_count: number;
      } | undefined;

      if (assemblerRan) {
        const key = assemblerRan.module_id;
        const existing = stats.get(key) ?? {
          module_id: key, recipe_id: assemblerRan.recipe_id, runs_in_window: 0,
          total_input_kg: 0, total_output_kg: 0, total_output_count: 0,
          last_run_tick: 0, throughput_per_hour: 0, utilization_pct: 0,
          stall_reason: null,
        };
        existing.runs_in_window++;
        existing.recipe_id = assemblerRan.recipe_id;
        existing.total_input_kg += assemblerRan.material_consumed_kg;
        existing.total_output_count += assemblerRan.component_produced_count;
        existing.last_run_tick = Math.max(existing.last_run_tick, evt.tick);
        stats.set(key, existing);
      }
    }

    // Compute derived fields
    const windowHours = (windowSize * minutesPerTick) / 60;
    for (const stat of stats.values()) {
      const outputTotal = stat.total_output_kg > 0 ? stat.total_output_kg : stat.total_output_count;
      stat.throughput_per_hour = windowHours > 0 ? outputTotal / windowHours : 0;
      stat.utilization_pct = windowSize > 0 ? (stat.runs_in_window / windowSize) * 100 : 0;
    }

    return stats;
  }, [events, currentTick, minutesPerTick, windowSize]);
}
