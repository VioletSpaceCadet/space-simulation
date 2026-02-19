import { useEffect, useReducer } from 'react'
import { createEventSource, fetchSnapshot } from '../api'
import type { AsteroidState, ResearchState, SimEvent, SimSnapshot } from '../types'

interface State {
  snapshot: SimSnapshot | null
  events: SimEvent[]
  connected: boolean
  currentTick: number
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
  research: ResearchState,
  events: SimEvent[],
): { asteroids: Record<string, AsteroidState>; research: ResearchState } {
  let updatedAsteroids = { ...asteroids }
  let updatedResearch = research

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
            knowledge: { tag_beliefs: [], composition: null },
          },
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
  }

  return { asteroids: updatedAsteroids, research: updatedResearch }
}

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case 'SNAPSHOT_LOADED':
      return { ...state, snapshot: action.snapshot, currentTick: action.snapshot.meta.tick }

    case 'EVENTS_RECEIVED': {
      const newEvents = [...action.events, ...state.events].slice(0, 500)
      const latestTick = action.events.reduce((max, e) => Math.max(max, e.tick), state.currentTick)
      if (!state.snapshot) return { ...state, events: newEvents, currentTick: latestTick }
      const { asteroids, research } = applyEvents(
        state.snapshot.asteroids,
        state.snapshot.research,
        action.events,
      )
      return {
        ...state,
        events: newEvents,
        currentTick: latestTick,
        snapshot: { ...state.snapshot, asteroids, research },
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
}

const RECONNECT_DELAY_MS = 2000
// Must be longer than heartbeat interval (5s) + flush interval (1s) with margin
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
          dispatch({ type: 'HEARTBEAT', tick: (data as { tick: number }).tick })
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

  return { snapshot: state.snapshot, events: state.events, connected: state.connected, currentTick: state.currentTick }
}
