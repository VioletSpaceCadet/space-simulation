import type { MetaInfo, SimSnapshot } from './types'

export async function fetchSnapshot(): Promise<SimSnapshot> {
  const response = await fetch('/api/v1/snapshot')
  if (!response.ok) throw new Error(`Snapshot fetch failed: ${response.status}`)
  return response.json()
}

export async function fetchMeta(): Promise<MetaInfo> {
  const response = await fetch('/api/v1/meta')
  if (!response.ok) throw new Error(`Meta fetch failed: ${response.status}`)
  return response.json()
}

export async function saveGame(): Promise<{ path: string; tick: number }> {
  const response = await fetch('/api/v1/save', { method: 'POST' })
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: 'unknown error' }))
    throw new Error(body.error ?? `Save failed: ${response.status}`)
  }
  return response.json()
}

export function createEventSource(): EventSource {
  return new EventSource('/api/v1/stream')
}
