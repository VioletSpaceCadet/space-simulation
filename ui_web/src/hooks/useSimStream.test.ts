import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as api from '../api'
import type { SimSnapshot } from '../types'
import { useSimStream } from './useSimStream'

const baseSnapshot: SimSnapshot = {
  meta: { tick: 5, seed: 42, content_version: '0.0.1' },
  scan_sites: [],
  asteroids: {},
  ships: {},
  stations: {},
  research: { unlocked: [], data_pool: {}, evidence: {} },
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
