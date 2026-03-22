import { CRYO_ELEMENTS, elementColor } from '../../config/theme';
import type {
  ComponentItem, InventoryItem, MaterialItem, ModuleItem,
  OreItem, SlagItem,
} from '../../types';

import { formatKg, totalInventoryKg } from './inventoryUtils';

const QUALITY_TIER_EXCELLENT = 0.8;
const QUALITY_TIER_GOOD = 0.5;

function qualityTier(quality: number): string {
  if (quality >= QUALITY_TIER_EXCELLENT) {return 'excellent';}
  if (quality >= QUALITY_TIER_GOOD) {return 'good';}
  return 'poor';
}

function pct(frac: number): string {
  return `${Math.round(frac * 100)}%`;
}

interface AggregatedOre {
  totalKg: number
  lotCount: number
  composition: Record<string, number>
}

function aggregateOre(inventory: InventoryItem[]): AggregatedOre | null {
  const oreLots = inventory.filter((i): i is OreItem => i.kind === 'Ore');
  if (oreLots.length === 0) {return null;}

  const totalKg = oreLots.reduce((sum, lot) => sum + lot.kg, 0);
  // Weighted-average composition
  const composition: Record<string, number> = {};
  for (const lot of oreLots) {
    for (const [el, frac] of Object.entries(lot.composition)) {
      composition[el] = (composition[el] ?? 0) + frac * lot.kg;
    }
  }
  for (const el of Object.keys(composition)) {
    composition[el] /= totalKg;
  }
  return { totalKg, lotCount: oreLots.length, composition };
}

export function InventoryDisplay({ inventory }: { inventory: InventoryItem[] }) {
  const hasModules = inventory.some((i) => i.kind === 'Module');
  const hasComponents = inventory.some((i) => i.kind === 'Component');
  const totalKg = totalInventoryKg(inventory);

  if (totalKg === 0 && !hasModules && !hasComponents) {return null;}

  const oreAgg = aggregateOre(inventory);
  const materials = inventory.filter((i) => i.kind === 'Material') as MaterialItem[];
  const slags = inventory.filter((i) => i.kind === 'Slag') as SlagItem[];
  const components = inventory.filter((i) => i.kind === 'Component') as ComponentItem[];
  const modules = inventory.filter((i) => i.kind === 'Module') as ModuleItem[];

  return (
    <div className="space-y-1 mt-0.5">
      {oreAgg && (
        <div className="text-cargo">
          ore {formatKg(oreAgg.totalKg)} kg
          <span className="text-faint ml-1">
            ({oreAgg.lotCount} lot{oreAgg.lotCount !== 1 ? 's' : ''},{' '}
            {Object.entries(oreAgg.composition)
              .sort(([, a], [, b]) => b - a)
              .filter(([, f]) => f > 0.001)
              .map(([el, f]) => `${el} ${pct(f)}`)
              .join(', ')})
          </span>
        </div>
      )}
      {materials.map((item, idx) => (
        <div key={`mat-${idx}`} className="flex items-center gap-1">
          <span style={{ color: elementColor(item.element) }}>
            {item.element} {formatKg(item.kg)} kg
          </span>
          <span className="text-faint">({qualityTier(item.quality)})</span>
          {CRYO_ELEMENTS.has(item.element) && (
            <span className="text-[9px] px-1 rounded text-amber-400/80 bg-amber-400/10">
              CRYO
            </span>
          )}
        </div>
      ))}
      {slags.length > 0 && (
        <div className="text-dim">
          slag {formatKg(slags.reduce((sum, s) => sum + s.kg, 0))} kg
        </div>
      )}
      {components.map((item, idx) => (
        <div key={`comp-${idx}`} className="text-cargo">
          {item.component_id} ×{item.count}
        </div>
      ))}
      {modules.map((item, idx) => (
        <div key={`mod-${idx}`} className="text-faint text-[10px]">
          module: {item.module_def_id}
        </div>
      ))}
    </div>
  );
}
