import { useCallback, useState } from 'react';

import type { PanelId } from '../layout';

export interface FloatingWindowState {
  id: string
  panelId: PanelId
  x: number
  y: number
  width: number
  height: number
  zIndex: number
}

const STORAGE_KEY = 'floating-windows';
const DEFAULT_WIDTH = 480;
const DEFAULT_HEIGHT = 360;

let nextZIndex = 100;

function loadWindows(): FloatingWindowState[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) { return []; }
    const parsed = JSON.parse(stored) as FloatingWindowState[];
    if (!Array.isArray(parsed)) { return []; }
    // Restore nextZIndex from persisted windows
    for (const win of parsed) {
      if (win.zIndex >= nextZIndex) { nextZIndex = win.zIndex + 1; }
    }
    return parsed;
  } catch {
    return [];
  }
}

function persistWindows(windows: FloatingWindowState[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(windows));
}

export function useFloatingWindows() {
  const [windows, setWindows] = useState<FloatingWindowState[]>(loadWindows);

  const openWindow = useCallback((panelId: PanelId, x?: number, y?: number) => {
    setWindows((current) => {
      // If already open, bring to front
      const existing = current.find(w => w.panelId === panelId);
      if (existing) {
        const z = nextZIndex++;
        const next = current.map(w =>
          w.id === existing.id ? { ...w, zIndex: z } : w,
        );
        persistWindows(next);
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
        zIndex: nextZIndex++,
      };
      const next = [...current, win];
      persistWindows(next);
      return next;
    });
  }, []);

  const closeWindow = useCallback((id: string) => {
    setWindows((current) => {
      const next = current.filter(w => w.id !== id);
      persistWindows(next);
      return next;
    });
  }, []);

  const updateWindow = useCallback((id: string, updates: Partial<FloatingWindowState>) => {
    setWindows((current) => {
      const next = current.map(w =>
        w.id === id ? { ...w, ...updates } : w,
      );
      persistWindows(next);
      return next;
    });
  }, []);

  const bringToFront = useCallback((id: string) => {
    setWindows((current) => {
      const z = nextZIndex++;
      const next = current.map(w =>
        w.id === id ? { ...w, zIndex: z } : w,
      );
      persistWindows(next);
      return next;
    });
  }, []);

  const closeWindowByPanel = useCallback((panelId: PanelId) => {
    setWindows((current) => {
      const next = current.filter(w => w.panelId !== panelId);
      persistWindows(next);
      return next;
    });
  }, []);

  return { windows, openWindow, closeWindow, updateWindow, bringToFront, closeWindowByPanel };
}
