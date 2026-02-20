import { describe, expect, it } from 'vitest'

describe('useSvgZoomPan', () => {
  it('exports without error', async () => {
    const mod = await import('./useSvgZoomPan')
    expect(mod.useSvgZoomPan).toBeDefined()
    expect(typeof mod.useSvgZoomPan).toBe('function')
  })
})
