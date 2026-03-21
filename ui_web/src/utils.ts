import type { OverheatZone, ThermalState } from './types';

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
  };
  return zoneColors[thermal.overheat_zone];
}
