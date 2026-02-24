import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../api'
import type { SimSnapshot } from '../types'
import { useSimStream } from './useSimStream'

const baseSnapshot: SimSnapshot = {
  meta: { tick: 5, seed: 42, content_version: '0.0.1' },
  scan_sites: [],
  asteroids: {},
  ships: {
    ship_0001: {
      id: 'ship_0001',
      location_node: 'node_earth_orbit',
      owner: 'principal_autopilot',
      cargo: {},
      cargo_capacity_m3: 20,
      task: null,
    },
  },
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
}

class MockEventSource {
  onopen: ((e: Event) => void) | null = null
  onerror: ((e: Event) => void) | null = null
  onmessage: ((e: MessageEvent) => void) | null = null
  close = vi.fn()
}

describe('useSimStream', () => {
  beforeEach(() => {
    vi.spyOn(api, 'fetchSnapshot').mockResolvedValue(baseSnapshot)
    vi.spyOn(api, 'createEventSource').mockReturnValue(new MockEventSource() as unknown as EventSource)
  })

  it('fetches snapshot on mount and sets tick', async () => {
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })
    expect(api.fetchSnapshot).toHaveBeenCalledOnce()
    expect(result.current.currentTick).toBe(5)
  })

  it('starts with empty events list', () => {
    const { result } = renderHook(() => useSimStream())
    expect(result.current.events).toEqual([])
  })

  it('resets state when watchdog fires after no data', async () => {
    vi.useFakeTimers()
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    // Simulate connection opening (starts watchdog)
    act(() => { mockEs.onopen!(new Event('open')) })
    expect(result.current.connected).toBe(true)

    // Advance past watchdog timeout with no messages
    await act(async () => { vi.advanceTimersByTime(3_100) })

    expect(result.current.connected).toBe(false)
    expect(result.current.snapshot).toBeNull()
    vi.useRealTimers()
  })

  it('resets state and retries on EventSource error', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })
    expect(result.current.currentTick).toBe(5)

    act(() => {
      mockEs.onerror!(new Event('error'))
    })

    expect(result.current.currentTick).toBe(0)
    expect(result.current.snapshot).toBeNull()
    expect(result.current.events).toEqual([])
  })

  it('closes EventSource on unmount', () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { unmount } = renderHook(() => useSimStream())
    unmount()
    expect(mockEs.close).toHaveBeenCalledOnce()
  })

  it('adds asteroid to table when AsteroidDiscovered event is received', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    const events = [
      { id: 'evt_000010', tick: 20, event: { AsteroidDiscovered: { asteroid_id: 'asteroid_0005', location_node: 'node_belt_inner' } } },
      { id: 'evt_000011', tick: 20, event: { ScanResult: { asteroid_id: 'asteroid_0005', tags: [['IronRich', 0.9]] } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    expect(result.current.snapshot?.asteroids['asteroid_0005']).toBeDefined()
    expect(result.current.snapshot?.asteroids['asteroid_0005'].location_node).toBe('node_belt_inner')
    expect(result.current.snapshot?.asteroids['asteroid_0005'].knowledge.tag_beliefs).toEqual([['IronRich', 0.9]])
  })

  it('updates currentTick when events are received', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    const events = [
      { id: 'evt_000001', tick: 42, event: { TaskStarted: { ship_id: 'ship_0001' } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    expect(result.current.currentTick).toBe(42)
  })

  it('updates ship task on TaskStarted', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    const events = [
      { id: 'evt_t1', tick: 10, event: { TaskStarted: { ship_id: 'ship_0001', task_kind: 'Survey', target: 'site_001' } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    const ship = result.current.snapshot?.ships['ship_0001']
    expect(ship?.task).not.toBeNull()
    expect(ship?.task?.kind).toHaveProperty('Survey')
  })

  it('clears ship task on TaskCompleted', async () => {
    const snapshotWithTask: SimSnapshot = {
      ...baseSnapshot,
      ships: {
        ship_0001: {
          ...baseSnapshot.ships['ship_0001'],
          task: { kind: { Survey: { site: 'site_001' } }, started_tick: 5, eta_tick: 15 },
        },
      },
    }
    vi.spyOn(api, 'fetchSnapshot').mockResolvedValue(snapshotWithTask)
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    expect(result.current.snapshot?.ships['ship_0001'].task).not.toBeNull()

    const events = [
      { id: 'evt_t2', tick: 15, event: { TaskCompleted: { ship_id: 'ship_0001', task_kind: 'Survey', target: 'site_001' } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    expect(result.current.snapshot?.ships['ship_0001'].task).toBeNull()
  })

  it('updates ship location on ShipArrived', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    expect(result.current.snapshot?.ships['ship_0001'].location_node).toBe('node_earth_orbit')

    const events = [
      { id: 'evt_s1', tick: 12, event: { ShipArrived: { ship_id: 'ship_0001', node: 'node_belt_inner' } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    expect(result.current.snapshot?.ships['ship_0001'].location_node).toBe('node_belt_inner')
  })

  it('accumulates data pool on DataGenerated', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    const events = [
      { id: 'evt_d1', tick: 10, event: { DataGenerated: { kind: 'ScanData', amount: 5.0 } } },
      { id: 'evt_d2', tick: 11, event: { DataGenerated: { kind: 'ScanData', amount: 3.0 } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    expect(result.current.snapshot?.research.data_pool['ScanData']).toBeCloseTo(8.0)
  })

  it('adds scan site on ScanSiteSpawned', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    expect(result.current.snapshot?.scan_sites).toEqual([])

    const events = [
      { id: 'evt_ss1', tick: 30, event: { ScanSiteSpawned: { site_id: 'site_new_001', node: 'node_belt_inner', template_id: 'tmpl_iron_rich' } } },
    ]
    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify(events) }))
    })

    expect(result.current.snapshot?.scan_sites).toHaveLength(1)
    expect(result.current.snapshot?.scan_sites[0]).toEqual({ id: 'site_new_001', node: 'node_belt_inner', template_id: 'tmpl_iron_rich' })
  })

  it('updates currentTick from heartbeat', async () => {
    const mockEs = new MockEventSource()
    vi.spyOn(api, 'createEventSource').mockReturnValue(mockEs as unknown as EventSource)
    const { result } = renderHook(() => useSimStream())
    await act(async () => { await Promise.resolve() })

    act(() => {
      mockEs.onmessage!(new MessageEvent('message', { data: JSON.stringify({ heartbeat: true, tick: 99 }) }))
    })

    expect(result.current.currentTick).toBe(99)
  })
})
