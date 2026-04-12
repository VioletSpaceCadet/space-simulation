/**
 * Pure selector for the CopilotKit hierarchical readable.
 *
 * Split out from `readables.ts` so vitest can import this module without
 * dragging the `@copilotkit/react-core/v2` package (and its side-effect
 * CSS imports) through Vite's transform pipeline. All hook-bound code
 * (useAgentContext registration) lives in `readables.ts`.
 *
 * See `readables.ts` for the full design doc + memoization rules.
 */

import type { ActiveAlert, SimSnapshot, StationState, ShipState } from '../types';

/** Maximum recent critical alerts surfaced in the top-level readable. */
const MAX_RECENT_CRITICAL_ALERTS = 3;

/** Maximum recent tech unlocks surfaced in the top-level readable. */
const MAX_RECENT_UNLOCKS = 5;

/**
 * The shape the LLM sees. Deliberately small — anything that can be
 * queried on demand belongs in a frontend tool or MCP call, not here.
 *
 * Keys use snake_case to match the convention the agent sees in its tool
 * parameter schemas (`section: "stations" | "ships" | ...`). This keeps
 * the model from guessing between camel/snake when composing tool calls.
 */
export interface SnapshotReadable {
  snapshot_tick: number;
  snapshot_age_label: string;
  paused: boolean;
  connected: boolean;
  treasury_usd: number;
  active_alerts: {
    total: number;
    warnings: number;
    critical: number;
    recent_critical: Array<{
      alert_id: string;
      message: string;
      suggested_action: string;
      tick: number;
    }>;
  };
  research: {
    unlocked_count: number;
    recent_unlocks: string[];
    data_pool_kinds: number;
    active_domains: number;
  };
  stations: {
    total: number;
    summary: Array<{
      id: string;
      module_count: number;
      crew_total: number;
      power_net_kw: number;
      avg_wear: number;
    }>;
  };
  fleet: {
    total: number;
    in_transit: number;
    mining: number;
    idle: number;
    other: number;
  };
  asteroids: {
    discovered: number;
    tagged: number;
  };
}

/**
 * Discriminate a `TaskKind` variant at runtime. The `TaskState['kind']`
 * type in `types.ts` models every Rust variant as an object wrapper
 * (`{ Mine: {...} }`), but serde's default external-tagging emits unit
 * variants like `Task::Idle` as the bare string `"Idle"`. The runtime
 * data we receive over SSE therefore contains a mix of strings and
 * objects, and `in` fails on strings. Handle both shapes here instead
 * of widening the shared type — other callers in the codebase still
 * read `kind` as an object and should not be forced to change.
 */
function isShipTask(
  ship: ShipState,
  kind: 'Idle' | 'Mine' | 'Transit' | 'Deposit' | 'Survey' | 'DeepScan',
): boolean {
  if (!ship.task) { return false; }
  const taskKind: unknown = ship.task.kind;
  if (typeof taskKind === 'string') { return taskKind === kind; }
  if (typeof taskKind === 'object' && taskKind !== null) {
    return kind in (taskKind as Record<string, unknown>);
  }
  return false;
}

function averageWear(modules: StationState['modules']): number {
  if (modules.length === 0) { return 0; }
  const total = modules.reduce((sum, m) => sum + m.wear.wear, 0);
  return Number((total / modules.length).toFixed(3));
}

function crewHeadcount(crew: Record<string, number> | undefined): number {
  if (!crew) { return 0; }
  return Object.values(crew).reduce((sum, n) => sum + n, 0);
}

function countAlertsBySeverity(alerts: Iterable<ActiveAlert>): { warnings: number; critical: number } {
  let warnings = 0;
  let critical = 0;
  for (const alert of alerts) {
    if (alert.severity === 'Critical') { critical++; }
    else if (alert.severity === 'Warning') { warnings++; }
  }
  return { warnings, critical };
}

function recentCriticalAlerts(
  alerts: Iterable<ActiveAlert>,
  limit: number,
): SnapshotReadable['active_alerts']['recent_critical'] {
  const critical: ActiveAlert[] = [];
  for (const alert of alerts) {
    if (alert.severity === 'Critical') { critical.push(alert); }
  }
  // Sort by tick descending so the most recent come first, then slice.
  critical.sort((a, b) => b.tick - a.tick);
  return critical.slice(0, limit).map((a) => ({
    alert_id: a.alert_id,
    message: a.message,
    suggested_action: a.suggested_action,
    tick: a.tick,
  }));
}

function countShipsByTask(ships: Record<string, ShipState>): {
  total: number;
  in_transit: number;
  mining: number;
  idle: number;
  other: number;
} {
  let in_transit = 0;
  let mining = 0;
  let idle = 0;
  let other = 0;
  const shipList = Object.values(ships);
  for (const ship of shipList) {
    if (!ship.task || isShipTask(ship, 'Idle')) { idle++; }
    else if (isShipTask(ship, 'Transit')) { in_transit++; }
    else if (isShipTask(ship, 'Mine')) { mining++; }
    else { other++; }
  }
  return { total: shipList.length, in_transit, mining, idle, other };
}

function summarizeStations(stations: Record<string, StationState>): SnapshotReadable['stations'] {
  const stationList = Object.values(stations);
  return {
    total: stationList.length,
    summary: stationList.map((s) => ({
      id: s.id,
      module_count: s.modules.length,
      crew_total: crewHeadcount(s.crew),
      power_net_kw: Number((s.power.generated_kw - s.power.consumed_kw).toFixed(1)),
      avg_wear: averageWear(s.modules),
    })),
  };
}

function summarizeResearch(snapshot: SimSnapshot): SnapshotReadable['research'] {
  const unlocked = snapshot.research.unlocked;
  return {
    unlocked_count: unlocked.length,
    recent_unlocks: unlocked.slice(-MAX_RECENT_UNLOCKS),
    data_pool_kinds: Object.keys(snapshot.research.data_pool).length,
    active_domains: Object.values(snapshot.research.evidence).filter(
      (d) => Object.keys(d.points).length > 0,
    ).length,
  };
}

function summarizeAsteroids(snapshot: SimSnapshot): SnapshotReadable['asteroids'] {
  const asteroidList = Object.values(snapshot.asteroids);
  return {
    discovered: asteroidList.length,
    tagged: asteroidList.filter((a) => a.anomaly_tags.length > 0).length,
  };
}

export interface SnapshotReadableInput {
  snapshot: SimSnapshot | null;
  activeAlerts: Map<string, ActiveAlert>;
  currentTick: number;
  paused: boolean;
  connected: boolean;
}

/**
 * Pure selector that flattens a live `SimSnapshot` + alert map into the
 * shape the LLM sees. Testable without a React tree.
 *
 * `snapshot` may be `null` during initial connection; returns a sparse
 * "offline" readable in that case so the LLM knows the sim isn't ready
 * yet instead of hallucinating state.
 */
export function buildSnapshotReadable(args: SnapshotReadableInput): SnapshotReadable {
  const { snapshot, activeAlerts, currentTick, paused, connected } = args;

  if (!snapshot) {
    return {
      snapshot_tick: currentTick,
      snapshot_age_label: connected ? 'loading initial snapshot…' : 'disconnected from sim_daemon',
      paused,
      connected,
      treasury_usd: 0,
      active_alerts: { total: 0, warnings: 0, critical: 0, recent_critical: [] },
      research: { unlocked_count: 0, recent_unlocks: [], data_pool_kinds: 0, active_domains: 0 },
      stations: { total: 0, summary: [] },
      fleet: { total: 0, in_transit: 0, mining: 0, idle: 0, other: 0 },
      asteroids: { discovered: 0, tagged: 0 },
    };
  }

  const alertsArray = Array.from(activeAlerts.values());
  const { warnings, critical } = countAlertsBySeverity(alertsArray);

  return {
    snapshot_tick: currentTick,
    snapshot_age_label: paused
      ? 'current (paused)'
      : `stale as of tick ${String(currentTick)} — recommend pausing for live-state queries`,
    paused,
    connected,
    treasury_usd: Math.round(snapshot.balance),
    active_alerts: {
      total: activeAlerts.size,
      warnings,
      critical,
      recent_critical: recentCriticalAlerts(alertsArray, MAX_RECENT_CRITICAL_ALERTS),
    },
    research: summarizeResearch(snapshot),
    stations: summarizeStations(snapshot.stations),
    fleet: countShipsByTask(snapshot.ships),
    asteroids: summarizeAsteroids(snapshot),
  };
}
