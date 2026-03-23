import type { OverheatZone, ShipState, ThermalState } from './types';

/**
 * Extract the discriminant key from a tagged-union event object.
 * E.g. `{ "ShipTaskCompleted": { ship_id: "s1" } }` → `"ShipTaskCompleted"`.
 */
export function getEventKey(event: Record<string, unknown>): string {
  return Object.keys(event)[0] ?? 'Unknown';
}

/**
 * Extract the task kind string from a ship's task.
 * Returns the discriminant key (e.g. "Transit", "Mine") or null if no task.
 */
export function getTaskKind(task: ShipState['task']): string | null {
  if (!task) { return null; }
  return Object.keys(task.kind)[0] ?? null;
}

export function formatCurrency(value: number): string {
  if (value >= 1_000_000_000) {
    return `$${(value / 1_000_000_000).toFixed(2)}B`;
  }
  if (value >= 1_000_000) {
    return `$${(value / 1_000_000).toFixed(1)}M`;
  }
  if (value >= 1_000) {
    return `$${(value / 1_000).toFixed(1)}K`;
  }
  return `$${value.toLocaleString('en-US', { maximumFractionDigits: 0 })}`;
}

const MILLIKELVIN_PER_KELVIN = 1000;
const KELVIN_TO_CELSIUS_OFFSET = 273.15;

export function formatTempMk(
  tempMk: number,
  unit: 'K' | 'C',
): string {
  const kelvin = tempMk / MILLIKELVIN_PER_KELVIN;
  if (unit === 'C') {
    return `${(kelvin - KELVIN_TO_CELSIUS_OFFSET).toFixed(1)} °C`;
  }
  return `${kelvin.toFixed(1)} K`;
}

export function thermalColorClass(
  thermal: ThermalState | undefined,
): string {
  if (!thermal) {return 'text-neutral-500';}
  if (thermal.overheat_disabled) {return 'text-red-500';}
  const zoneColors: Record<OverheatZone, string> = {
    Nominal: 'text-emerald-500',
    Warning: 'text-amber-500',
    Critical: 'text-red-500',
    Damage: 'text-red-500',
  };
  return zoneColors[thermal.overheat_zone];
}

export function formatQty(qty: number): string {
  if (qty >= 1000) { return `${(qty / 1000).toFixed(1)}k`; }
  return qty.toFixed(qty < 10 ? 1 : 0);
}
