import type { InventoryItem } from '../../types';

export function formatKg(kg: number): string {
  return kg.toLocaleString(undefined, { maximumFractionDigits: 1 });
}

export function totalInventoryKg(inventory: InventoryItem[]): number {
  return inventory.reduce((sum, i) => sum + ('kg' in i ? (i as { kg: number }).kg : 0), 0);
}
