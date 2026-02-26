import { act, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { useAnimatedTick } from './useAnimatedTick';

describe('useAnimatedTick', () => {
  let rafCallbacks: ((time: number) => void)[];
  let rafId: number;

  beforeEach(() => {
    rafCallbacks = [];
    rafId = 0;
    vi.spyOn(window, 'requestAnimationFrame').mockImplementation((cb) => {
      rafCallbacks.push(cb);
      return ++rafId;
    });
    vi.spyOn(window, 'cancelAnimationFrame').mockImplementation(() => {});
    vi.spyOn(performance, 'now').mockReturnValue(0);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  function flushRaf(time: number) {
    vi.spyOn(performance, 'now').mockReturnValue(time);
    const cbs = [...rafCallbacks];
    rafCallbacks = [];
    cbs.forEach((cb) => cb(time));
  }

  it('returns serverTick initially before any rAF fires', () => {
    const { result } = renderHook(() => useAnimatedTick(100, 10));
    expect(result.current.displayTick).toBe(100);
  });

  it('does not interpolate with only one server sample', () => {
    const { result } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 100, rate: 10 } },
    );

    // Only 1 sample — should stay at anchor tick, not interpolate forward
    act(() => { flushRaf(100); });
    expect(result.current.displayTick).toBe(100);
  });

  it('interpolates forward after receiving two server updates', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 100, rate: 10 } },
    );

    // Provide a second sample so interpolation activates
    vi.spyOn(performance, 'now').mockReturnValue(100);
    rerender({ serverTick: 101, rate: 10 });

    // Simulate 100ms after second sample (should advance ~1 tick at 10 ticks/sec)
    act(() => { flushRaf(200); });
    expect(result.current.displayTick).toBeCloseTo(102, 0);
  });

  it('snaps to serverTick when a new server value arrives', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 100, rate: 10 } },
    );

    // Advance 50ms
    act(() => { flushRaf(50); });

    // Server says tick is now 105
    rerender({ serverTick: 105, rate: 10 });
    act(() => { flushRaf(51); });

    expect(result.current.displayTick).toBeGreaterThanOrEqual(105);
  });

  it('measures tick rate from server samples', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 0, rate: 10 } },
    );

    // Feed several server tick updates at 20 ticks/sec (50ms per tick)
    for (let tick = 1; tick <= 5; tick++) {
      vi.spyOn(performance, 'now').mockReturnValue(tick * 50);
      rerender({ serverTick: tick, rate: 10 });
    }

    // measuredTickRate should be close to 20
    expect(result.current.measuredTickRate).toBeGreaterThan(15);
    expect(result.current.measuredTickRate).toBeLessThan(25);
  });

  it('does not advance displayTick beyond reasonable bound', () => {
    const { result, rerender } = renderHook(
      ({ serverTick, rate }) => useAnimatedTick(serverTick, rate),
      { initialProps: { serverTick: 100, rate: 10 } },
    );

    // Need 2 samples to enable interpolation
    vi.spyOn(performance, 'now').mockReturnValue(100);
    rerender({ serverTick: 101, rate: 10 });

    // Simulate a huge time jump (2 seconds after anchor)
    act(() => { flushRaf(2100); });

    // Max lookahead is 1 second = 10 ticks from anchor (101), so ≤ 111
    expect(result.current.displayTick).toBeLessThanOrEqual(115);
  });

  it('cancels rAF on unmount', () => {
    const { unmount } = renderHook(() => useAnimatedTick(100, 10));
    unmount();
    expect(window.cancelAnimationFrame).toHaveBeenCalled();
  });
});
