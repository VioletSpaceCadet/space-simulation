import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useFloatingWindows } from './useFloatingWindows';

describe('useFloatingWindows', () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
    vi.spyOn(Storage.prototype, 'setItem');
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.restoreAllMocks();
  });

  it('starts with empty windows when nothing persisted', () => {
    const { result } = renderHook(() => useFloatingWindows());
    expect(result.current.windows).toEqual([]);
  });

  it('opens a floating window', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map'); });

    expect(result.current.windows).toHaveLength(1);
    expect(result.current.windows[0].panelId).toBe('map');
    expect(result.current.windows[0].width).toBe(480);
    expect(result.current.windows[0].height).toBe(360);
  });

  it('brings existing window to front instead of opening duplicate', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map'); });
    const initialZ = result.current.windows[0].zIndex;

    act(() => { result.current.openWindow('map'); });

    expect(result.current.windows).toHaveLength(1);
    expect(result.current.windows[0].zIndex).toBeGreaterThan(initialZ);
  });

  it('closes a window by id', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('events'); });
    const windowId = result.current.windows[0].id;

    act(() => { result.current.closeWindow(windowId); });

    expect(result.current.windows).toHaveLength(0);
  });

  it('closes a window by panelId', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('fleet'); });
    expect(result.current.windows).toHaveLength(1);

    act(() => { result.current.closeWindowByPanel('fleet'); });
    expect(result.current.windows).toHaveLength(0);
  });

  it('updates window position and size', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map'); });
    const windowId = result.current.windows[0].id;

    act(() => {
      result.current.updateWindow(windowId, { x: 200, y: 300 });
    });

    expect(result.current.windows[0].x).toBe(200);
    expect(result.current.windows[0].y).toBe(300);

    act(() => {
      result.current.updateWindow(windowId, { width: 600, height: 400 });
    });

    expect(result.current.windows[0].width).toBe(600);
    expect(result.current.windows[0].height).toBe(400);
  });

  it('brings a window to front', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map'); });
    act(() => { result.current.openWindow('events'); });

    const mapWindow = result.current.windows.find(w => w.panelId === 'map')!;
    const eventsWindow = result.current.windows.find(w => w.panelId === 'events')!;
    expect(eventsWindow.zIndex).toBeGreaterThan(mapWindow.zIndex);

    act(() => { result.current.bringToFront(mapWindow.id); });

    const updatedMap = result.current.windows.find(w => w.panelId === 'map')!;
    const updatedEvents = result.current.windows.find(w => w.panelId === 'events')!;
    expect(updatedMap.zIndex).toBeGreaterThan(updatedEvents.zIndex);
  });

  it('persists to localStorage', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map'); });

    expect(localStorage.setItem).toHaveBeenCalledWith(
      'floating-windows',
      expect.any(String),
    );
  });

  it('restores from localStorage', () => {
    const saved = [{
      id: 'float-map-123',
      panelId: 'map',
      x: 50, y: 50,
      width: 500, height: 400,
      zIndex: 100,
    }];
    localStorage.setItem('floating-windows', JSON.stringify(saved));

    const { result } = renderHook(() => useFloatingWindows());

    expect(result.current.windows).toHaveLength(1);
    expect(result.current.windows[0].panelId).toBe('map');
    expect(result.current.windows[0].x).toBe(50);
  });

  it('handles corrupted localStorage gracefully', () => {
    localStorage.setItem('floating-windows', 'not-json');

    const { result } = renderHook(() => useFloatingWindows());

    expect(result.current.windows).toEqual([]);
  });

  it('opens window at custom position', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map', 250, 150); });

    expect(result.current.windows[0].x).toBe(250);
    expect(result.current.windows[0].y).toBe(150);
  });

  it('filters out invalid entries from localStorage', () => {
    const saved = [
      { id: 'ok', panelId: 'map', x: 0, y: 0, width: 480, height: 360, zIndex: 100 },
      { id: 'bad', panelId: 'nonexistent', x: 0, y: 0, width: 480, height: 360, zIndex: 101 },
      { id: 'incomplete' },
    ];
    localStorage.setItem('floating-windows', JSON.stringify(saved));

    const { result } = renderHook(() => useFloatingWindows());

    expect(result.current.windows).toHaveLength(1);
    expect(result.current.windows[0].panelId).toBe('map');
  });

  it('debounces persistence for updateWindow', () => {
    const { result } = renderHook(() => useFloatingWindows());

    act(() => { result.current.openWindow('map'); });
    const callsBefore = (localStorage.setItem as ReturnType<typeof vi.fn>).mock.calls.length;

    act(() => { result.current.updateWindow(result.current.windows[0].id, { x: 200 }); });
    act(() => { result.current.updateWindow(result.current.windows[0].id, { x: 250 }); });

    // Not yet persisted (debounced)
    const callsAfterUpdates = (localStorage.setItem as ReturnType<typeof vi.fn>).mock.calls.length;
    expect(callsAfterUpdates).toBe(callsBefore);

    // Flush the debounce timer
    act(() => { vi.advanceTimersByTime(300); });

    const callsAfterFlush = (localStorage.setItem as ReturnType<typeof vi.fn>).mock.calls.length;
    expect(callsAfterFlush).toBeGreaterThan(callsBefore);
  });
});
