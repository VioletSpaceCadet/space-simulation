import type { MetaInfo, SimSnapshot } from './types';

export async function fetchSnapshot(): Promise<SimSnapshot> {
  const response = await fetch('/api/v1/snapshot');
  if (!response.ok) {throw new Error(`Snapshot fetch failed: ${response.status}`);}
  return response.json();
}

export async function fetchMeta(): Promise<MetaInfo> {
  const response = await fetch('/api/v1/meta');
  if (!response.ok) {throw new Error(`Meta fetch failed: ${response.status}`);}
  return response.json();
}

export async function saveGame(): Promise<{ path: string; tick: number }> {
  const response = await fetch('/api/v1/save', { method: 'POST' });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: 'unknown error' }));
    throw new Error(body.error ?? `Save failed: ${response.status}`);
  }
  return response.json();
}

export async function pauseGame(): Promise<{ paused: boolean }> {
  const response = await fetch('/api/v1/pause', { method: 'POST' });
  if (!response.ok) {throw new Error(`Pause failed: ${response.status}`);}
  return response.json();
}

export async function resumeGame(): Promise<{ paused: boolean }> {
  const response = await fetch('/api/v1/resume', { method: 'POST' });
  if (!response.ok) {throw new Error(`Resume failed: ${response.status}`);}
  return response.json();
}

export async function setSpeed(ticksPerSec: number): Promise<{ ticks_per_sec: number }> {
  const response = await fetch('/api/v1/speed', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ticks_per_sec: ticksPerSec }),
  });
  if (!response.ok) {throw new Error(`Speed change failed: ${response.status}`);}
  return response.json();
}

export function createEventSource(): EventSource {
  return new EventSource('/api/v1/stream');
}
