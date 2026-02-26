import { renderHook, act } from '@testing-library/react';
import { describe, it, expect, beforeEach } from 'vitest';

import { buildDefaultLayout, ALL_PANELS, serializeLayout, findPanelIds } from '../layout';
import type { GroupNode } from '../layout';

import { useLayoutState } from './useLayoutState';

const STORAGE_KEY = 'panel-layout';

describe('useLayoutState', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it('starts with default horizontal layout when no localStorage', () => {
    const { result } = renderHook(() => useLayoutState());
    const expected = buildDefaultLayout(ALL_PANELS);
    expect(result.current.layout).toEqual(expected);
    expect(result.current.visiblePanels).toEqual(ALL_PANELS);
  });

  it('restores layout from localStorage', () => {
    const custom: GroupNode = {
      type: 'group',
      direction: 'vertical',
      children: [
        { type: 'leaf', panelId: 'map' },
        { type: 'leaf', panelId: 'fleet' },
      ],
    };
    localStorage.setItem(STORAGE_KEY, serializeLayout(custom));

    const { result } = renderHook(() => useLayoutState());
    expect(result.current.layout).toEqual(custom);
    expect(result.current.visiblePanels).toEqual(['map', 'fleet']);
  });

  it('persists layout changes to localStorage', () => {
    const { result } = renderHook(() => useLayoutState());

    act(() => {
      result.current.togglePanel('research');
    });

    const stored = localStorage.getItem(STORAGE_KEY);
    expect(stored).not.toBeNull();
    const parsed = JSON.parse(stored!);
    const ids = findPanelIds(parsed);
    expect(ids).not.toContain('research');
  });

  it('togglePanel removes a visible panel', () => {
    const { result } = renderHook(() => useLayoutState());
    expect(result.current.visiblePanels).toContain('events');

    act(() => {
      result.current.togglePanel('events');
    });

    expect(result.current.visiblePanels).not.toContain('events');
  });

  it('togglePanel adds back a hidden panel', () => {
    const { result } = renderHook(() => useLayoutState());

    act(() => {
      result.current.togglePanel('events');
    });
    expect(result.current.visiblePanels).not.toContain('events');

    act(() => {
      result.current.togglePanel('events');
    });
    expect(result.current.visiblePanels).toContain('events');
  });

  it('does not remove last visible panel', () => {
    // Start with a layout that has only one panel
    const single: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [{ type: 'leaf', panelId: 'map' }],
    };
    localStorage.setItem(STORAGE_KEY, serializeLayout(single));

    const { result } = renderHook(() => useLayoutState());
    expect(result.current.visiblePanels).toEqual(['map']);

    act(() => {
      result.current.togglePanel('map');
    });

    // Should still have map visible
    expect(result.current.visiblePanels).toEqual(['map']);
  });

  it('move repositions a panel', () => {
    const { result } = renderHook(() => useLayoutState());

    act(() => {
      result.current.move('research', 'map', 'before');
    });

    const ids = result.current.visiblePanels;
    expect(ids).toContain('research');
    expect(ids).toContain('map');
    // research should now be before map
    expect(ids.indexOf('research')).toBeLessThan(ids.indexOf('map'));
  });

  it('resetLayout restores default layout', () => {
    const { result } = renderHook(() => useLayoutState());

    act(() => {
      result.current.togglePanel('events');
      result.current.togglePanel('fleet');
    });
    expect(result.current.visiblePanels).not.toContain('events');

    act(() => {
      result.current.resetLayout();
    });

    expect(result.current.layout).toEqual(buildDefaultLayout(ALL_PANELS));
    expect(result.current.visiblePanels).toEqual(ALL_PANELS);
  });

  it('falls back to default layout for invalid localStorage', () => {
    localStorage.setItem(STORAGE_KEY, 'not valid json {{{');
    const { result } = renderHook(() => useLayoutState());
    expect(result.current.layout).toEqual(buildDefaultLayout(ALL_PANELS));
  });
});
