import type { ContentResponse, MetaInfo, SimSnapshot, SolarSystemConfig } from './types';

export const API_PATHS = {
  snapshot: '/api/v1/snapshot',
  meta: '/api/v1/meta',
  save: '/api/v1/save',
  pause: '/api/v1/pause',
  resume: '/api/v1/resume',
  speed: '/api/v1/speed',
  spatialConfig: '/api/v1/spatial-config',
  content: '/api/v1/content',
  stream: '/api/v1/stream',
  pricing: '/api/v1/pricing',
  command: '/api/v1/command',
} as const;

export async function fetchSnapshot(): Promise<SimSnapshot> {
  const response = await fetch(API_PATHS.snapshot);
  if (!response.ok) {throw new Error(`Snapshot fetch failed: ${response.status}`);}
  return response.json();
}

export async function fetchMeta(): Promise<MetaInfo> {
  const response = await fetch(API_PATHS.meta);
  if (!response.ok) {throw new Error(`Meta fetch failed: ${response.status}`);}
  return response.json();
}

export async function saveGame(): Promise<{ path: string; tick: number }> {
  const response = await fetch(API_PATHS.save, { method: 'POST' });
  if (!response.ok) {
    const body = await response.json().catch(() => ({ error: 'unknown error' })); // intentional — fallback when response body is not JSON
    throw new Error(body.error ?? `Save failed: ${response.status}`);
  }
  return response.json();
}

export async function pauseGame(): Promise<{ paused: boolean }> {
  const response = await fetch(API_PATHS.pause, { method: 'POST' });
  if (!response.ok) {throw new Error(`Pause failed: ${response.status}`);}
  return response.json();
}

export async function resumeGame(): Promise<{ paused: boolean }> {
  const response = await fetch(API_PATHS.resume, { method: 'POST' });
  if (!response.ok) {throw new Error(`Resume failed: ${response.status}`);}
  return response.json();
}

export async function setSpeed(ticksPerSec: number): Promise<{ ticks_per_sec: number }> {
  const response = await fetch(API_PATHS.speed, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ ticks_per_sec: ticksPerSec }),
  });
  if (!response.ok) {throw new Error(`Speed change failed: ${response.status}`);}
  return response.json();
}

export async function fetchSpatialConfig(): Promise<SolarSystemConfig> {
  const response = await fetch(API_PATHS.spatialConfig);
  if (!response.ok) {throw new Error(`Spatial config fetch failed: ${response.status}`);}
  return response.json();
}

export async function fetchContent(): Promise<ContentResponse> {
  const response = await fetch(API_PATHS.content);
  if (!response.ok) { throw new Error(`Content fetch failed: ${response.status}`); }
  return response.json();
}

export function createEventSource(): EventSource {
  return new EventSource(API_PATHS.stream);
}
