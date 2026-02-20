import { useEffect, useReducer } from 'react'
import { createEventSource, fetchSnapshot } from '../api'
import type { ResearchState, SimEvent, SimSnapshot } from '../types'
import { applyEvents } from './applyEvents'

// Kept for backward compatibility with SolarSystemMap/DetailCard imports.
// Composition is now embedded in InventoryItem::Ore; this type is unused in new code.
export type OreCompositions = Record<string, Record<string, number>>

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

const initialState: State = {
  snapshot: null,
  events: [],
  connected: false,
  currentTick: 0,
}

function reducer(state: State, action: Action): State {
  switch (action.type) {
    case 'SNAPSHOT_LOADED':
      return { ...state, snapshot: action.snapshot, currentTick: action.snapshot.meta.tick }

    case 'EVENTS_RECEIVED': {
      const newEvents = [...action.events, ...state.events].slice(0, 500)
      const latestTick = action.events.reduce((max, e) => Math.max(max, e.tick), state.currentTick)
      if (!state.snapshot) return { ...state, events: newEvents, currentTick: latestTick }
      const { asteroids, ships, stations, research, scanSites } = applyEvents(
        state.snapshot.asteroids,
        state.snapshot.ships,
        state.snapshot.stations,
        state.snapshot.research,
        state.snapshot.scan_sites,
        action.events,
      )
      return {
        ...state,
        events: newEvents,
        currentTick: latestTick,
        snapshot: { ...state.snapshot, asteroids, ships, stations, research, scan_sites: scanSites },
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

const RECONNECT_DELAY_MS = 2000
// Must be longer than heartbeat interval (200ms) with generous margin
const WATCHDOG_MS = 3_000

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
  }
}
