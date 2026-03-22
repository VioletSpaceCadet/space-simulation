import type { ModuleState } from '../../types';
import { formatTempMk, thermalColorClass } from '../../utils';

const WEAR_TIER_HIGH = 0.8;
const WEAR_TIER_MED = 0.5;

function wearColor(wear: number): string {
  if (wear >= WEAR_TIER_HIGH) {return 'text-red-400';}
  if (wear >= WEAR_TIER_MED) {return 'text-yellow-400';}
  return 'text-green-400';
}

export function ModuleCard({
  module: m,
  tempUnit,
}: {
  module: ModuleState
  tempUnit: 'K' | 'C'
}) {
  const name = m.def_id.replace(/^module_/, '');
  const healthPct = m.wear ? Math.round((1 - m.wear.wear) * 100) : 100;
  const ks = m.kind_state;
  const processor = typeof ks === 'object' && 'Processor' in ks
    ? ks.Processor : null;
  const assembler = typeof ks === 'object' && 'Assembler' in ks
    ? ks.Assembler : null;
  const isMaintenance = typeof ks === 'object' && 'Maintenance' in ks;
  const isStalled = (processor?.stalled) || (assembler?.stalled);

  const tempDisplay = m.thermal
    ? formatTempMk(m.thermal.temp_mk, tempUnit)
    : 'N/A';
  const tempColor = thermalColorClass(m.thermal);

  return (
    <div className="border border-edge rounded px-2 py-1.5 bg-surface/30">
      <div className="flex items-center gap-2">
        <span className="text-fg">{name}</span>
        <span className={
          'text-[9px] px-1 rounded '
          + (m.enabled
            ? 'text-online bg-online/10'
            : 'text-offline bg-offline/10')
        }>
          {m.enabled ? 'ON' : 'OFF'}
        </span>
        {isStalled && (
          <span className="text-[9px] px-1 rounded text-red-400 bg-red-400/10">
            STALLED
          </span>
        )}
        {m.thermal?.overheat_disabled && (
          <span className="text-[9px] px-1 rounded text-red-500 bg-red-500/10">
            OVERHEAT
          </span>
        )}
        {m.thermal && !m.thermal.overheat_disabled && m.thermal.overheat_zone === 'Warning' && (
          <span className="text-[9px] px-1 rounded text-amber-400 bg-amber-400/10">
            HOT
          </span>
        )}
        {m.thermal && !m.thermal.overheat_disabled && m.thermal.overheat_zone === 'Critical' && (
          <span className="text-[9px] px-1 rounded text-red-400 bg-red-400/10">
            CRITICAL
          </span>
        )}
        {m.thermal && m.thermal.overheat_zone === 'Damage' && (
          <span className="text-[9px] px-1 rounded text-red-300 bg-red-500/20 font-bold">
            DAMAGE
          </span>
        )}
      </div>
      <div className="flex items-center gap-2 mt-1 text-[10px]">
        <span className="text-dim">health</span>
        <span className={
          m.wear ? wearColor(m.wear.wear) : 'text-green-400'
        }>
          {healthPct}%
        </span>
        <span className="text-dim ml-2">temp</span>
        <span data-testid="temp-readout" className={tempColor}>
          {tempDisplay}
        </span>
        {processor && (
          <span className="text-faint ml-2">
            threshold {processor.threshold_kg} kg
          </span>
        )}
        {isMaintenance && (
          <span className="text-faint ml-2">maintenance bay</span>
        )}
        {assembler && (
          <span className="text-faint ml-2">assembler</span>
        )}
      </div>
    </div>
  );
}
