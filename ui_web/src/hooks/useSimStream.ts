import { useEffect, useReducer } from 'react'
import { createEventSource, fetchSnapshot } from '../api'
import type { AsteroidState, ResearchState, ShipState, SimEvent, SimSnapshot, StationState, TaskState } from '../types'

// Maps "ore:{asteroid_id}" cargo keys to composition fractions (0–1).
// Populated from OreMined events; persists even after the asteroid is depleted.
export type OreCompositions = Record<string, Record<string, number>>

interface State {
  snapshot: SimSnapshot | null
  events: SimEvent[]
  connected: boolean
  currentTick: number
  oreCompositions: OreCompositions
}

function buildTaskStub(taskKind: string, target: string | null, tick: number): TaskState {
  const kindMap: Record<string, Record<string, unknown>> = {
    Survey: target ? { Survey: { site: target } } : { Idle: {} },
    DeepScan: target ? { DeepScan: { asteroid: target } } : { Idle: {} },
    Mine: target ? { Mine: { asteroid: target, duration_ticks: 0 } } : { Idle: {} },
    Deposit: target ? { Deposit: { station: target } } : { Idle: {} },
    Transit: target ? { Transit: { destination: target, total_ticks: 0 } } : { Idle: {} },
  }
  return {
    kind: (kindMap[taskKind] ?? { Idle: {} }) as TaskState['kind'],
    started_tick: tick,
    eta_tick: 0,
  }
}

function addToRecord(base: Record<string, number>, additions: Record<string, number>): Record<string, number> {
  const result = { ...base }
  for (const [key, val] of Object.entries(additions)) {
    result[key] = (result[key] ?? 0) + val
  }
  return result
}

type Action =
  | { type: 'SNAPSHOT_LOADED'; snapshot: SimSnapshot }
  | { type: 'EVENTS_RECEIVED'; events: SimEvent[] }
  | { type: 'HEARTBEAT'; tick: number }
  | { type: 'CONNECTED' }
  | { type: 'DISCONNECTED' }
  | { type: 'RESET' }

function applyEvents(
  asteroids: Record<string, AsteroidState>,
  ships: Record<string, ShipState>,
  stations: Record<string, StationState>,
  research: ResearchState,
  oreCompositions: OreCompositions,
  events: SimEvent[],
): {
  asteroids: Record<string, AsteroidState>
  ships: Record<string, ShipState>
  stations: Record<string, StationState>
  research: ResearchState
  oreCompositions: OreCompositions
} {
  let updatedAsteroids = { ...asteroids }
  let updatedShips = { ...ships }
  let updatedStations = { ...stations }
  let updatedResearch = research
  let updatedOreCompositions = oreCompositions

  for (const evt of events) {
    const e = evt.event

    if (e['AsteroidDiscovered']) {
      const { asteroid_id, location_node } = e['AsteroidDiscovered'] as { asteroid_id: string; location_node: string }
      if (!updatedAsteroids[asteroid_id]) {
        updatedAsteroids = {
          ...updatedAsteroids,
          [asteroid_id]: {
            id: asteroid_id,
            location_node,
            anomaly_tags: [],
            // mass_kg intentionally omitted — unknown until snapshot or OreMined event
            knowledge: { tag_beliefs: [], composition: null },
          },
        }
      }
    }

    if (e['OreMined']) {
      const { ship_id, asteroid_id, extracted, asteroid_remaining_kg } = e['OreMined'] as {
        ship_id: string
        asteroid_id: string
        extracted: Record<string, number>
        asteroid_remaining_kg: number
      }
      // Update asteroid remaining mass, or remove it if fully depleted
      if (updatedAsteroids[asteroid_id]) {
        if (asteroid_remaining_kg <= 0) {
          updatedAsteroids = Object.fromEntries(
            Object.entries(updatedAsteroids).filter(([id]) => id !== asteroid_id)
          )
        } else {
          updatedAsteroids = {
            ...updatedAsteroids,
            [asteroid_id]: { ...updatedAsteroids[asteroid_id], mass_kg: asteroid_remaining_kg },
          }
        }
      }
      // Add extracted ore to ship cargo (key is "ore:{asteroid_id}")
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: {
            ...updatedShips[ship_id],
            cargo: addToRecord(updatedShips[ship_id].cargo, extracted),
          },
        }
      }
      // Record composition for each ore lot key. Cache it now so the
      // composition survives even if the asteroid is later depleted and removed.
      const composition = updatedAsteroids[asteroid_id]?.knowledge.composition
      if (composition) {
        for (const oreKey of Object.keys(extracted)) {
          if (oreKey.startsWith('ore:') && !updatedOreCompositions[oreKey]) {
            updatedOreCompositions = { ...updatedOreCompositions, [oreKey]: composition }
          }
        }
      }
    }

    if (e['OreDeposited']) {
      const { ship_id, station_id, deposited } = e['OreDeposited'] as {
        ship_id: string
        station_id: string
        deposited: Record<string, number>
      }
      // Add each ore lot to station cargo separately (no blending)
      if (updatedStations[station_id]) {
        updatedStations = {
          ...updatedStations,
          [station_id]: {
            ...updatedStations[station_id],
            cargo: addToRecord(updatedStations[station_id].cargo, deposited),
          },
        }
      }
      // Clear ship cargo
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: { ...updatedShips[ship_id], cargo: {} },
        }
      }
    }

    if (e['ScanResult']) {
      const { asteroid_id, tags } = e['ScanResult'] as { asteroid_id: string; tags: [string, number][] }
      if (updatedAsteroids[asteroid_id]) {
        updatedAsteroids = {
          ...updatedAsteroids,
          [asteroid_id]: {
            ...updatedAsteroids[asteroid_id],
            knowledge: { ...updatedAsteroids[asteroid_id].knowledge, tag_beliefs: tags },
          },
        }
      }
    }

    if (e['CompositionMapped']) {
      const { asteroid_id, composition } = e['CompositionMapped'] as { asteroid_id: string; composition: Record<string, number> }
      if (updatedAsteroids[asteroid_id]) {
        updatedAsteroids = {
          ...updatedAsteroids,
          [asteroid_id]: {
            ...updatedAsteroids[asteroid_id],
            knowledge: { ...updatedAsteroids[asteroid_id].knowledge, composition },
          },
        }
      }
    }

    if (e['TechUnlocked']) {
      const { tech_id } = e['TechUnlocked'] as { tech_id: string }
      updatedResearch = {
        ...updatedResearch,
        unlocked: [...updatedResearch.unlocked, tech_id],
      }
    }

    if (e['TaskStarted']) {
      const { ship_id, task_kind, target } = e['TaskStarted'] as {
        ship_id: string
        task_kind: string
        target: string | null
      }
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: {
            ...updatedShips[ship_id],
            task: buildTaskStub(task_kind, target, evt.tick),
          },
        }
      }
    }

    if (e['TaskCompleted']) {
      const { ship_id } = e['TaskCompleted'] as { ship_id: string }
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: { ...updatedShips[ship_id], task: null },
        }
      }
    }

    if (e['ShipArrived']) {
      const { ship_id, node } = e['ShipArrived'] as { ship_id: string; node: string }
      if (updatedShips[ship_id]) {
        updatedShips = {
          ...updatedShips,
          [ship_id]: { ...updatedShips[ship_id], location_node: node },
        }
      }
    }

    if (e['DataGenerated']) {
      const { kind, amount } = e['DataGenerated'] as { kind: string; amount: number }
      updatedResearch = {
        ...updatedResearch,
        data_pool: {
          ...updatedResearch.data_pool,
          [kind]: (updatedResearch.data_pool[kind] ?? 0) + amount,
        },
      }
    }
  }

  return {
    asteroids: updatedAsteroids,
    ships: updatedShips,
    stations: updatedStations,
    research: updatedResearch,
    oreCompositions: updatedOreCompositions,
  }
}

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case 'SNAPSHOT_LOADED':
      return { ...state, snapshot: action.snapshot, currentTick: action.snapshot.meta.tick }

    case 'EVENTS_RECEIVED': {
      const newEvents = [...action.events, ...state.events].slice(0, 500)
      const latestTick = action.events.reduce((max, e) => Math.max(max, e.tick), state.currentTick)
      if (!state.snapshot) return { ...state, events: newEvents, currentTick: latestTick }
      const { asteroids, ships, stations, research, oreCompositions } = applyEvents(
        state.snapshot.asteroids,
        state.snapshot.ships,
        state.snapshot.stations,
        state.snapshot.research,
        state.oreCompositions,
        action.events,
      )
      return {
        ...state,
        events: newEvents,
        currentTick: latestTick,
        snapshot: { ...state.snapshot, asteroids, ships, stations, research },
        oreCompositions,
      }
    }

    case 'HEARTBEAT':
      return { ...state, currentTick: action.tick }

    case 'CONNECTED':
      return { ...state, connected: true }

    case 'DISCONNECTED':
      return { ...state, connected: false }

    case 'RESET':
      return initialState

    default:
      return state
  }
}

const initialState: State = {
  snapshot: null,
  events: [],
  connected: false,
  currentTick: 0,
  oreCompositions: {},
}

const RECONNECT_DELAY_MS = 2000
// Must be longer than heartbeat interval (5s) with margin
const WATCHDOG_MS = 10_000

export function useSimStream() {
  const [state, dispatch] = useReducer(reducer, initialState)

  useEffect(() => {
    let stopped = false
    let currentEs: EventSource | null = null
    let retryTimer: ReturnType<typeof setTimeout> | null = null
    let watchdogTimer: ReturnType<typeof setTimeout> | null = null

    function clearWatchdog() {
      if (watchdogTimer !== null) {
        clearTimeout(watchdogTimer)
        watchdogTimer = null
      }
    }

    function resetWatchdog() {
      clearWatchdog()
      watchdogTimer = setTimeout(() => {
        if (stopped) return
        // No data from daemon in WATCHDOG_MS — treat as disconnect
        dispatch({ type: 'RESET' })
        currentEs?.close()
        currentEs = null
        scheduleRetry()
      }, WATCHDOG_MS)
    }

    function scheduleRetry() {
      if (stopped || retryTimer !== null) return
      retryTimer = setTimeout(() => {
        retryTimer = null
        if (!stopped) connect()
      }, RECONNECT_DELAY_MS)
    }

    function connect() {
      fetchSnapshot()
        .then((snapshot) => { if (!stopped) dispatch({ type: 'SNAPSHOT_LOADED', snapshot }) })
        .catch(scheduleRetry)

      const es = createEventSource()
      currentEs = es

      es.onopen = () => {
        if (!stopped) {
          dispatch({ type: 'CONNECTED' })
          resetWatchdog()
        }
      }

      es.onerror = () => {
        if (stopped) return
        clearWatchdog()
        dispatch({ type: 'RESET' })
        es.close()
        if (currentEs === es) currentEs = null
        scheduleRetry()
      }

      es.onmessage = (event: MessageEvent) => {
        if (stopped) return
        resetWatchdog()
        const data = JSON.parse(event.data as string) as unknown
        if (data && typeof data === 'object' && 'heartbeat' in data) {
          dispatch({ type: 'HEARTBEAT', tick: (data as unknown as { tick: number }).tick })
        } else if (Array.isArray(data)) {
          dispatch({ type: 'EVENTS_RECEIVED', events: data as SimEvent[] })
        }
      }
    }

    connect()

    return () => {
      stopped = true
      clearWatchdog()
      currentEs?.close()
      if (retryTimer !== null) {
        clearTimeout(retryTimer)
        retryTimer = null
      }
    }
  }, [])

  return {
    snapshot: state.snapshot,
    events: state.events,
    connected: state.connected,
    currentTick: state.currentTick,
    oreCompositions: state.oreCompositions,
  }
}
