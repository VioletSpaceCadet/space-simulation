import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fetchMeta, fetchSnapshot, saveGame, setSpeed } from './api'

describe('fetchSnapshot', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('calls /api/v1/snapshot', async () => {
    const mock = { meta: { tick: 0, seed: 42, content_version: 'test' }, scan_sites: [], asteroids: {}, ships: {}, stations: {}, research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} } }
    vi.mocked(global.fetch).mockResolvedValueOnce(new Response(JSON.stringify(mock)))
    await fetchSnapshot()
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/snapshot')
  })

  it('returns parsed snapshot with correct tick', async () => {
    const mock = { meta: { tick: 42, seed: 1, content_version: '0.0.1' }, scan_sites: [], asteroids: {}, ships: {}, stations: {}, research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} } }
    vi.mocked(global.fetch).mockResolvedValueOnce(new Response(JSON.stringify(mock)))
    const result = await fetchSnapshot()
    expect(result.meta.tick).toBe(42)
  })
})

describe('fetchMeta', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('calls /api/v1/meta', async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce(new Response(JSON.stringify({ tick: 0, seed: 1, content_version: 'test' })))
    await fetchMeta()
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/meta')
  })

  it('returns parsed meta with ticks_per_sec', async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ tick: 0, seed: 1, content_version: 'test', ticks_per_sec: 50 }))
    )
    const result = await fetchMeta()
    expect(result.ticks_per_sec).toBe(50)
  })
})

describe('saveGame', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('sends POST to /api/v1/save', async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ path: 'runs/test/saves/save_0.json', tick: 0 }))
    )
    await saveGame()
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/save', { method: 'POST' })
  })

  it('throws on error response', async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ error: 'no run directory' }), { status: 503 })
    )
    await expect(saveGame()).rejects.toThrow('no run directory')
  })
})

describe('setSpeed', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('sends POST to /api/v1/speed with ticks_per_sec', async () => {
    vi.mocked(global.fetch).mockResolvedValueOnce(
      new Response(JSON.stringify({ ticks_per_sec: 1000 }))
    )
    await setSpeed(1000)
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/speed', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ ticks_per_sec: 1000 }),
    })
  })
})
