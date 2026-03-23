import type { HullDef, ShipState } from '../../types';
import { getTaskKind } from '../../utils';

import { InventoryDisplay } from './InventoryDisplay';
import { TaskProgress } from './TaskProgress';

interface ShipDetailProps {
  ship: ShipState
  hulls: Record<string, HullDef>
  displayTick: number
}

export function ShipDetail({ ship, hulls, displayTick }: ShipDetailProps) {
  const task = ship.task;
  const taskType = getTaskKind(task);
  const hull = ship.hull_id ? hulls[ship.hull_id] : undefined;
  const fittedModules = ship.fitted_modules ?? [];

  return (
    <div className="grid grid-cols-[auto_auto_auto] gap-x-8 gap-y-2 text-[11px] w-fit">
      {/* Left: Hull + Fitted modules */}
      <div>
        <div className="text-label text-[10px] uppercase tracking-wider mb-1">Hull</div>
        <div className="text-fg mb-2">{hull?.name ?? ship.hull_id ?? 'unknown'}</div>
        {hull && (
          <div className="space-y-0.5 text-dim text-[10px] mb-2">
            <div>Cargo: {ship.cargo_capacity_m3.toFixed(0)} m³</div>
            {ship.propellant_kg != null && ship.propellant_capacity_kg != null && (
              <div>Propellant: {ship.propellant_kg.toFixed(0)} / {ship.propellant_capacity_kg.toFixed(0)} kg</div>
            )}
          </div>
        )}
        <div className="text-label text-[10px] uppercase tracking-wider mb-1">Fitted Modules</div>
        {fittedModules.length === 0 ? (
          <span className="text-faint">none</span>
        ) : (
          <div className="space-y-0.5">
            {fittedModules.map((fm) => {
              const slotLabel = hull?.slots[fm.slot_index]?.label ?? `Slot ${fm.slot_index}`;
              return (
                <div key={fm.slot_index} className="text-dim">
                  <span className="text-fg">{fm.module_def_id}</span>
                  <span className="text-faint ml-1">({slotLabel})</span>
                </div>
              );
            })}
          </div>
        )}
      </div>
      {/* Middle: Inventory */}
      <div>
        <div className="text-label text-[10px] uppercase tracking-wider mb-1">Cargo</div>
        {ship.inventory.length === 0 ? (
          <span className="text-faint">empty</span>
        ) : (
          <InventoryDisplay inventory={ship.inventory} />
        )}
      </div>
      {/* Right: Task detail */}
      <div>
        <div className="text-label text-[10px] uppercase tracking-wider mb-1">Task</div>
        {!task ? (
          <span className="text-faint">idle</span>
        ) : (
          <div className="space-y-1">
            <div className="text-fg">{taskType}</div>
            <TaskProgress task={task} displayTick={displayTick} />
            {taskType === 'Transit' && 'Transit' in task.kind && (
              <div className="text-dim">destination: {task.kind.Transit.destination.parent_body}</div>
            )}
            {taskType === 'Mine' && 'Mine' in task.kind && (
              <div className="text-dim">asteroid: {task.kind.Mine.asteroid}</div>
            )}
            {taskType === 'Deposit' && 'Deposit' in task.kind && (
              <>
                <div className="text-dim">station: {task.kind.Deposit.station}</div>
                {task.kind.Deposit.blocked && (
                  <span className="text-[9px] px-1 rounded text-red-400 bg-red-400/10">BLOCKED</span>
                )}
              </>
            )}
            {taskType === 'Survey' && 'Survey' in task.kind && (
              <div className="text-dim">site: {task.kind.Survey.site}</div>
            )}
            {taskType === 'DeepScan' && 'DeepScan' in task.kind && (
              <div className="text-dim">asteroid: {task.kind.DeepScan.asteroid}</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
