import { useCallback, useEffect, useRef, useState } from 'react';

import { ALL_PANELS, type PanelId } from '../layout';

export interface FloatingWindowState {
  id: string
  panelId: PanelId
  x: number
  y: number
  width: number
  height: number
  zIndex: number
}

/** Fields that callers may update (position/size only). */
export type FloatingWindowUpdates = Pick<
  Partial<FloatingWindowState>, 'x' | 'y' | 'width' | 'height'
>;

const STORAGE_KEY = 'floating-windows';
const DEFAULT_WIDTH = 480;
const DEFAULT_HEIGHT = 360;
const VALID_PANELS = new Set<string>(ALL_PANELS);

function isValidWindow(win: unknown): win is FloatingWindowState {
  if (typeof win !== 'object' || win === null) { return false; }
  const record = win as Record<string, unknown>;
  return (
    typeof record.id === 'string' &&
    typeof record.panelId === 'string' &&
    VALID_PANELS.has(record.panelId) &&
    typeof record.x === 'number' &&
    typeof record.y === 'number' &&
    typeof record.width === 'number' &&
    typeof record.height === 'number' &&
    typeof record.zIndex === 'number'
  );
}

function loadWindows(): FloatingWindowState[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) { return []; }
    const parsed = JSON.parse(stored) as unknown[];
    if (!Array.isArray(parsed)) { return []; }
    return parsed.filter(isValidWindow);
  } catch {
    return [];
  }
}

function deriveNextZ(windows: FloatingWindowState[]): number {
  let max = 100;
  for (const win of windows) {
    if (win.zIndex >= max) { max = win.zIndex + 1; }
  }
  return max;
}

export function useFloatingWindows() {
  const [windows, setWindows] = useState<FloatingWindowState[]>(loadWindows);
  const nextZRef = useRef(deriveNextZ(windows));

  // Debounced persistence — write to localStorage at most once per 200ms
  const persistTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latestWindowsRef = useRef(windows);
  useEffect(() => { latestWindowsRef.current = windows; }, [windows]);

  const schedulePersist = useCallback(() => {
    if (persistTimerRef.current) { return; }
    persistTimerRef.current = setTimeout(() => {
      persistTimerRef.current = null;
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify(latestWindowsRef.current),
      );
    }, 200);
  }, []);

  // Persist immediately on structural changes (open/close)
  const persistNow = useCallback((wins: FloatingWindowState[]) => {
    if (persistTimerRef.current) {
      clearTimeout(persistTimerRef.current);
      persistTimerRef.current = null;
    }
    localStorage.setItem(STORAGE_KEY, JSON.stringify(wins));
  }, []);

  // Flush pending persist on unmount
  useEffect(() => () => {
    if (persistTimerRef.current) {
      clearTimeout(persistTimerRef.current);
      localStorage.setItem(
        STORAGE_KEY,
        JSON.stringify(latestWindowsRef.current),
      );
    }
  }, []);

  const openWindow = useCallback((panelId: PanelId, x?: number, y?: number) => {
    setWindows((current) => {
      // If already open, bring to front
      const existing = current.find(w => w.panelId === panelId);
      if (existing) {
        const z = nextZRef.current++;
        const next = current.map(w =>
          w.id === existing.id ? { ...w, zIndex: z } : w,
        );
        persistNow(next);
        return next;
      }
      const id = `float-${panelId}-${Date.now()}`;
      const win: FloatingWindowState = {
        id,
        panelId,
        x: x ?? 100 + current.length * 30,
        y: y ?? 100 + current.length * 30,
        width: DEFAULT_WIDTH,
        height: DEFAULT_HEIGHT,
        zIndex: nextZRef.current++,
      };
      const next = [...current, win];
      persistNow(next);
      return next;
    });
  }, [persistNow]);

  const closeWindow = useCallback((id: string) => {
    setWindows((current) => {
      const next = current.filter(w => w.id !== id);
      persistNow(next);
      return next;
    });
  }, [persistNow]);

  const updateWindow = useCallback((id: string, updates: FloatingWindowUpdates) => {
    setWindows((current) =>
      current.map(w => (w.id === id ? { ...w, ...updates } : w)),
    );
    schedulePersist();
  }, [schedulePersist]);

  const bringToFront = useCallback((id: string) => {
    setWindows((current) => {
      const z = nextZRef.current++;
      return current.map(w =>
        w.id === id ? { ...w, zIndex: z } : w,
      );
    });
    schedulePersist();
  }, [schedulePersist]);

  const closeWindowByPanel = useCallback((panelId: PanelId) => {
    setWindows((current) => {
      const next = current.filter(w => w.panelId !== panelId);
      persistNow(next);
      return next;
    });
  }, [persistNow]);

  return {
    windows, openWindow, closeWindow,
    updateWindow, bringToFront, closeWindowByPanel,
  };
}
