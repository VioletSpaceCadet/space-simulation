import { beforeEach, describe, expect, it, vi } from 'vitest'
import { fetchMeta, fetchSnapshot } from './api'

describe('fetchSnapshot', () => {
  beforeEach(() => {
    global.fetch = vi.fn()
  })

  it('calls /api/v1/snapshot', async () => {
    const mock = { meta: { tick: 0, seed: 42, content_version: 'test' }, scan_sites: [], asteroids: {}, ships: {}, stations: {}, research: { unlocked: [], data_pool: {}, evidence: {} } }
    vi.mocked(global.fetch).mockResolvedValueOnce(new Response(JSON.stringify(mock)))
    await fetchSnapshot()
    expect(global.fetch).toHaveBeenCalledWith('/api/v1/snapshot')
  })

  it('returns parsed snapshot with correct tick', async () => {
    const mock = { meta: { tick: 42, seed: 1, content_version: '0.0.1' }, scan_sites: [], asteroids: {}, ships: {}, stations: {}, research: { unlocked: [], data_pool: {}, evidence: {} } }
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
})
