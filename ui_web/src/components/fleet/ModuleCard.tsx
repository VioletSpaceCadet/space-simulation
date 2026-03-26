import type { ModuleState, ThermalState } from '../../types';
import { formatTempMk, thermalColorClass } from '../../utils';

const WEAR_TIER_HIGH = 0.8;
const WEAR_TIER_MED = 0.5;

function wearColor(wear: number): string {
  if (wear >= WEAR_TIER_HIGH) {return 'text-red-400';}
  if (wear >= WEAR_TIER_MED) {return 'text-yellow-400';}
  return 'text-green-400';
}

const BADGE = 'text-[9px] px-1 rounded';

function StatusBadges({
  isStalled,
  thermal,
  crewSatisfied,
}: {
  isStalled: boolean
  thermal?: ThermalState
  crewSatisfied?: boolean
}) {
  return (
    <>
      {isStalled && (
        <span className={`${BADGE} text-red-400 bg-red-400/10`}>STALLED</span>
      )}
      {thermal?.overheat_disabled && (
        <span className={`${BADGE} text-red-500 bg-red-500/10`}>OVERHEAT</span>
      )}
      {thermal && !thermal.overheat_disabled && thermal.overheat_zone === 'Warning' && (
        <span className={`${BADGE} text-amber-400 bg-amber-400/10`}>HOT</span>
      )}
      {thermal && !thermal.overheat_disabled && thermal.overheat_zone === 'Critical' && (
        <span className={`${BADGE} text-red-400 bg-red-400/10`}>CRITICAL</span>
      )}
      {thermal && thermal.overheat_zone === 'Damage' && (
        <span className={`${BADGE} text-red-300 bg-red-500/20 font-bold`}>DAMAGE</span>
      )}
      {crewSatisfied === false && (
        <span className={`${BADGE} text-orange-400 bg-orange-400/10`}>UNDERSTAFFED</span>
      )}
    </>
  );
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
  const isStalled = Boolean(processor?.stalled) || Boolean(assembler?.stalled);

  const tempDisplay = m.thermal
    ? formatTempMk(m.thermal.temp_mk, tempUnit)
    : 'N/A';
  const tempColor = thermalColorClass(m.thermal);

  return (
    <div className="border border-edge rounded px-2 py-1.5 bg-surface/30">
      <div className="flex items-center gap-2">
        <span className="text-fg">{name}</span>
        <span className={
          `${BADGE} `
          + (m.enabled
            ? 'text-online bg-online/10'
            : 'text-offline bg-offline/10')
        }>
          {m.enabled ? 'ON' : 'OFF'}
        </span>
        <StatusBadges
          isStalled={isStalled}
          thermal={m.thermal}
          crewSatisfied={m.crew_satisfied}
        />
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
        {m.assigned_crew && Object.keys(m.assigned_crew).length > 0 && (
          <span className="text-faint ml-2">
            crew: {Object.entries(m.assigned_crew).map(([role, count]) => `${count} ${role}`).join(', ')}
          </span>
        )}
      </div>
    </div>
  );
}
