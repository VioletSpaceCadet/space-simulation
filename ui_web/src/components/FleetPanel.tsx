import { useState } from 'react';

import { useContent } from '../hooks/useContent';
import type { HullDef, PowerState, ShipState, StationState } from '../types';
import { getTaskKind } from '../utils';

import { type ColumnDef, ExpandableTable } from './ExpandableTable';
import { InventoryDisplay } from './fleet/InventoryDisplay';
import { formatKg, totalInventoryKg } from './fleet/inventoryUtils';
import { ModuleCard } from './fleet/ModuleCard';
import { ShipDetail } from './fleet/ShipDetail';
import { TaskProgress } from './fleet/TaskProgress';

function taskLabel(task: ShipState['task']): string {
  if (!task) {return 'idle';}
  const key = getTaskKind(task) ?? 'idle';
  return key.toLowerCase();
}

// --- Ship table ---

interface ShipRow {
  id: string
  hull: string
  parent_body: string
  task: string
  cargo_kg: number
  ship: ShipState
}

function ShipsTable({ ships, displayTick, hulls }: { ships: ShipState[]; displayTick: number; hulls: Record<string, HullDef> }) {
  const rows: ShipRow[] = ships.map((ship) => ({
    id: ship.id,
    hull: ship.hull_id ? (hulls[ship.hull_id]?.name ?? ship.hull_id) : 'unknown',
    parent_body: ship.position.parent_body,
    task: taskLabel(ship.task),
    cargo_kg: totalInventoryKg(ship.inventory),
    ship,
  }));

  const columns: ColumnDef<ShipRow>[] = [
    { key: 'id', label: 'ID', render: (r) => r.id },
    { key: 'hull', label: 'Hull', render: (r) => r.hull },
    { key: 'parent_body', label: 'Location', render: (r) => r.parent_body },
    { key: 'task', label: 'Task', render: (r) => r.task },
    { key: 'progress', label: 'Progress', sortable: false, render: (r) => <TaskProgress task={r.ship.task} displayTick={displayTick} /> },
    { key: 'cargo_kg', label: 'Cargo', render: (r) => r.cargo_kg === 0
      ? <span className="text-faint">empty</span>
      : <span className="text-cargo">{formatKg(r.cargo_kg)} kg</span>
    },
  ];

  return (
    <ExpandableTable
      data={rows}
      columns={columns}
      renderDetail={(r) => <ShipDetail ship={r.ship} hulls={hulls} displayTick={displayTick} />}
    />
  );
}

// --- Station detail components ---

function PowerBar({ power }: { power: PowerState }) {
  const { generated_kw, consumed_kw, deficit_kw, battery_stored_kwh } = power;
  const usagePct = generated_kw > 0 ? Math.min(consumed_kw / generated_kw, 1) : (consumed_kw > 0 ? 1 : 0);
  const hasDeficit = deficit_kw > 0;

  return (
    <div className="text-[11px]">
      <div className="text-label text-[10px] uppercase tracking-wider mb-1">Power</div>
      <div className="flex items-center gap-2 mb-1">
        <div className="flex-1 h-2 bg-surface rounded overflow-hidden">
          <div
            className={`h-full rounded transition-all ${hasDeficit ? 'bg-red-400' : 'bg-green-400'}`}
            style={{ width: `${Math.round(usagePct * 100)}%` }}
          />
        </div>
        <span className="text-dim whitespace-nowrap">
          {consumed_kw.toFixed(0)} / {generated_kw.toFixed(0)} kW
        </span>
      </div>
      {hasDeficit && (
        <div className="text-red-400 text-[10px]">
          Deficit: {deficit_kw.toFixed(0)} kW — modules stalled
        </div>
      )}
      {battery_stored_kwh > 0 && (
        <div className="text-dim text-[10px]">
          Battery: {battery_stored_kwh.toFixed(1)} kWh
        </div>
      )}
    </div>
  );
}

function StationDetail({ station }: { station: StationState }) {
  const [tempUnit, setTempUnit] = useState<'K' | 'C'>('K');
  const hasThermal = station.modules.some((m) => m.thermal);

  return (
    <div className="space-y-3 text-[11px] w-fit">
      <PowerBar power={station.power} />
      <div className="grid grid-cols-[auto_auto] gap-x-8 gap-y-2">
        <div>
          <div className="text-label text-[10px] uppercase tracking-wider mb-1">
            Inventory
          </div>
          {station.inventory.length === 0 ? (
            <span className="text-faint">empty</span>
          ) : (
            <InventoryDisplay inventory={station.inventory} />
          )}
        </div>
        <div>
          <div className="flex items-center gap-2 mb-1">
            <span className="text-label text-[10px] uppercase tracking-wider">
              Modules
            </span>
            {hasThermal && (
              <button
                type="button"
                onClick={(event) => {
                  event.stopPropagation();
                  setTempUnit((u) => (u === 'K' ? 'C' : 'K'));
                }}
                className={
                  'text-[9px] px-1 rounded cursor-pointer '
                  + 'text-dim hover:text-fg bg-surface/50'
                }
                aria-label={`Switch to °${tempUnit === 'K' ? 'C' : 'K'}`}
              >
                °{tempUnit}
              </button>
            )}
          </div>
          {station.modules.length === 0 ? (
            <span className="text-faint">none installed</span>
          ) : (
            <div className="space-y-2">
              {station.modules.map((m) => (
                <ModuleCard
                  key={m.id}
                  module={m}
                  tempUnit={tempUnit}
                />
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// --- Station table ---

interface StationRow {
  id: string
  parent_body: string
  cargo_kg: number
  station: StationState
}

function StationsTable({ stations }: { stations: StationState[] }) {
  const rows: StationRow[] = stations.map((station) => ({
    id: station.id,
    parent_body: station.position.parent_body,
    cargo_kg: totalInventoryKg(station.inventory),
    station,
  }));

  const columns: ColumnDef<StationRow>[] = [
    { key: 'id', label: 'ID', render: (r) => r.id },
    { key: 'parent_body', label: 'Location', render: (r) => r.parent_body },
    { key: 'cargo_kg', label: 'Storage', render: (r) => r.cargo_kg === 0
      ? <span className="text-faint">empty</span>
      : <span className="text-cargo">{formatKg(r.cargo_kg)} kg</span>
    },
  ];

  return (
    <ExpandableTable
      data={rows}
      columns={columns}
      renderDetail={(r) => <StationDetail station={r.station} />}
    />
  );
}

// --- Main panel ---

interface Props {
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  displayTick: number
}

export function FleetPanel({ ships, stations, displayTick }: Props) {
  const { content } = useContent();
  const hulls = content?.hulls ?? {};
  const shipRows = Object.values(ships);
  const stationRows = Object.values(stations);

  return (
    <div className="overflow-y-auto flex-1">
      {shipRows.length === 0 ? (
        <div className="text-faint italic py-1">no ships</div>
      ) : (
        <ShipsTable ships={shipRows} displayTick={displayTick} hulls={hulls} />
      )}

      <div className="text-[10px] uppercase tracking-widest text-label mt-3 mb-1.5 pb-1 border-b border-edge">
        Stations
      </div>

      {stationRows.length === 0 ? (
        <div className="text-faint italic py-1">no stations</div>
      ) : (
        <StationsTable stations={stationRows} />
      )}
    </div>
  );
}
