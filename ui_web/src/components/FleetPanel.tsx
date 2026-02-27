import React, { useState } from 'react';

import { useSortableData } from '../hooks/useSortableData';
import type { ComponentItem, InventoryItem, MaterialItem, ModuleItem, ModuleState, OreItem, PowerState, ShipState, SlagItem, StationState } from '../types';

import { SortIndicator } from './SortIndicator';

const QUALITY_TIER_EXCELLENT = 0.8;
const QUALITY_TIER_GOOD = 0.5;
const WEAR_TIER_HIGH = 0.8;
const WEAR_TIER_MED = 0.5;

function qualityTier(quality: number): string {
  if (quality >= QUALITY_TIER_EXCELLENT) {return 'excellent';}
  if (quality >= QUALITY_TIER_GOOD) {return 'good';}
  return 'poor';
}

function pct(frac: number): string {
  return `${Math.round(frac * 100)}%`;
}

function formatKg(kg: number): string {
  return kg.toLocaleString(undefined, { maximumFractionDigits: 1 });
}

function taskLabel(task: ShipState['task']): string {
  if (!task) {return 'idle';}
  const key = Object.keys(task.kind)[0] ?? 'idle';
  return key.toLowerCase();
}

function totalInventoryKg(inventory: InventoryItem[]): number {
  return inventory.reduce((sum, i) => sum + ('kg' in i ? (i as { kg: number }).kg : 0), 0);
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

function InventoryDisplay({ inventory }: { inventory: InventoryItem[] }) {
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
        <div key={`mat-${idx}`} className="text-cargo">
          {item.element} {formatKg(item.kg)} kg
          <span className="text-faint ml-1">({qualityTier(item.quality)})</span>
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

function wearColor(wear: number): string {
  if (wear >= WEAR_TIER_HIGH) {return 'text-red-400';}
  if (wear >= WEAR_TIER_MED) {return 'text-yellow-400';}
  return 'text-green-400';
}

function TaskProgress({ task, displayTick }: { task: ShipState['task']; displayTick: number }) {
  if (!task) {return null;}
  const total = task.eta_tick - task.started_tick;
  if (total <= 0) {return null;}
  const elapsed = Math.max(0, Math.min(displayTick - task.started_tick, total));
  const pctDone = Math.round((elapsed / total) * 100);

  return (
    <div className="flex items-center gap-1.5 min-w-[80px]">
      <div
        role="progressbar"
        aria-valuenow={pctDone}
        aria-valuemin={0}
        aria-valuemax={100}
        className="flex-1 h-1.5 bg-edge rounded-full overflow-hidden"
      >
        <div
          className="h-full bg-accent rounded-full"
          style={{ width: `${pctDone}%` }}
        />
      </div>
      <span className="text-muted text-[10px] w-7 text-right">{pctDone}%</span>
    </div>
  );
}

// --- Ship detail ---

function ShipDetail({ ship, displayTick }: { ship: ShipState; displayTick: number }) {
  const task = ship.task;
  const taskType = task ? Object.keys(task.kind)[0] : null;

  return (
    <div className="grid grid-cols-[auto_auto] gap-x-8 gap-y-2 text-[11px] w-fit">
      {/* Left: Inventory */}
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
              <div className="text-dim">destination: {task.kind.Transit.destination}</div>
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

// --- Expandable table ---

const HEADER_CLASS =
  'text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none';
const HEADER_CLASS_STATIC =
  'text-left text-label px-2 py-1 border-b border-edge font-normal select-none';
const CELL_CLASS = 'px-2 py-0.5 border-b border-surface';

interface ColumnDef<T> {
  key: string
  label: string
  sortable?: boolean
  render: (row: T) => React.ReactNode
}

function ExpandableTable<T extends { id: string }>({
  data,
  columns,
  renderDetail,
}: {
  data: T[]
  columns: ColumnDef<T>[]
  renderDetail: (row: T) => React.ReactNode
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const sortableRows = data.map((row) => {
    const sortable: Record<string, unknown> = { id: row.id };
    for (const col of columns) {sortable[col.key] = col.key === 'id' ? row.id : (row as Record<string, unknown>)[col.key];}
    return { ...sortable, _row: row };
  });

  const { sortedData, sortConfig, requestSort: requestSortTyped } = useSortableData(sortableRows);
  const requestSort = requestSortTyped as (key: string) => void;
  const colSpan = columns.length;

  return (
    <table className="w-full border-collapse text-[11px]">
      <thead>
        <tr>
          {columns.map((col) => (
            <th
              key={col.key}
              className={col.sortable !== false ? HEADER_CLASS : HEADER_CLASS_STATIC}
              onClick={col.sortable !== false ? () => requestSort(col.key) : undefined}
            >
              {col.label}
              {col.sortable !== false && <SortIndicator column={col.key} sortConfig={sortConfig} />}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {sortedData.map((sortableRow) => {
          const row = sortableRow._row as T;
          const isExpanded = expandedId === row.id;
          return (
            <React.Fragment key={row.id}>
              <tr
                className={`cursor-pointer hover:bg-surface/50 ${isExpanded ? 'bg-surface/60' : ''}`}
                onClick={() => setExpandedId(isExpanded ? null : row.id)}
              >
                {columns.map((col, colIndex) => (
                  <td
                    key={col.key}
                    className={`${CELL_CLASS} ${colIndex === 0 && isExpanded ? 'border-l-2 border-l-accent' : ''}`}
                  >
                    {col.render(row)}
                  </td>
                ))}
              </tr>
              {isExpanded && (
                <tr>
                  <td colSpan={colSpan} className="px-3 py-3 border-b border-surface border-l-2 border-l-accent bg-void/30">
                    {renderDetail(row)}
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

// --- Ship table ---

interface ShipRow {
  id: string
  location_node: string
  task: string
  cargo_kg: number
  ship: ShipState
}

function ShipsTable({ ships, displayTick }: { ships: ShipState[]; displayTick: number }) {
  const rows: ShipRow[] = ships.map((ship) => ({
    id: ship.id,
    location_node: ship.location_node,
    task: taskLabel(ship.task),
    cargo_kg: totalInventoryKg(ship.inventory),
    ship,
  }));

  const columns: ColumnDef<ShipRow>[] = [
    { key: 'id', label: 'ID', render: (r) => r.id },
    { key: 'location_node', label: 'Location', render: (r) => r.location_node },
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
      renderDetail={(r) => <ShipDetail ship={r.ship} displayTick={displayTick} />}
    />
  );
}

// --- Station detail components ---

function ModuleCard({ module: m }: { module: ModuleState }) {
  const name = m.def_id.replace(/^module_/, '');
  const healthPct = m.wear ? Math.round((1 - m.wear.wear) * 100) : 100;
  const ks = m.kind_state;
  const processor = typeof ks === 'object' && 'Processor' in ks ? ks.Processor : null;
  const assembler = typeof ks === 'object' && 'Assembler' in ks ? ks.Assembler : null;
  const isMaintenance = typeof ks === 'object' && 'Maintenance' in ks;
  const isStalled = (processor?.stalled) || (assembler?.stalled);

  return (
    <div className="border border-edge rounded px-2 py-1.5 bg-surface/30">
      <div className="flex items-center gap-2">
        <span className="text-fg">{name}</span>
        <span className={`text-[9px] px-1 rounded ${m.enabled ? 'text-online bg-online/10' : 'text-offline bg-offline/10'}`}>
          {m.enabled ? 'ON' : 'OFF'}
        </span>
        {isStalled && (
          <span className="text-[9px] px-1 rounded text-red-400 bg-red-400/10">STALLED</span>
        )}
      </div>
      <div className="flex items-center gap-2 mt-1 text-[10px]">
        <span className="text-dim">health</span>
        <span className={m.wear ? wearColor(m.wear.wear) : 'text-green-400'}>{healthPct}%</span>
        {processor && (
          <span className="text-faint ml-2">threshold {processor.threshold_kg} kg</span>
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
  return (
    <div className="space-y-3 text-[11px] w-fit">
      {station.power && <PowerBar power={station.power} />}
      <div className="grid grid-cols-[auto_auto] gap-x-8 gap-y-2">
        <div>
          <div className="text-label text-[10px] uppercase tracking-wider mb-1">Inventory</div>
          {station.inventory.length === 0 ? (
            <span className="text-faint">empty</span>
          ) : (
            <InventoryDisplay inventory={station.inventory} />
          )}
        </div>
        <div>
          <div className="text-label text-[10px] uppercase tracking-wider mb-1">Modules</div>
          {station.modules.length === 0 ? (
            <span className="text-faint">none installed</span>
          ) : (
            <div className="space-y-2">
              {station.modules.map((m) => (
                <ModuleCard key={m.id} module={m} />
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
  location_node: string
  cargo_kg: number
  station: StationState
}

function StationsTable({ stations }: { stations: StationState[] }) {
  const rows: StationRow[] = stations.map((station) => ({
    id: station.id,
    location_node: station.location_node,
    cargo_kg: totalInventoryKg(station.inventory),
    station,
  }));

  const columns: ColumnDef<StationRow>[] = [
    { key: 'id', label: 'ID', render: (r) => r.id },
    { key: 'location_node', label: 'Location', render: (r) => r.location_node },
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
  const shipRows = Object.values(ships);
  const stationRows = Object.values(stations);

  return (
    <div className="overflow-y-auto flex-1">
      {shipRows.length === 0 ? (
        <div className="text-faint italic py-1">no ships</div>
      ) : (
        <ShipsTable ships={shipRows} displayTick={displayTick} />
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
