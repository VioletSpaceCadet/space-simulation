import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import { FloatingWindow } from './FloatingWindow';

const defaults = {
  id: 'float-1',
  panelId: 'map' as const,
  x: 100,
  y: 100,
  width: 480,
  height: 360,
  zIndex: 100,
  onClose: vi.fn(),
  onUpdate: vi.fn(),
  onFocus: vi.fn(),
  onDock: vi.fn(),
};

describe('FloatingWindow', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('renders with correct position and size', () => {
    render(
      <FloatingWindow {...defaults}>
        <div>content</div>
      </FloatingWindow>,
    );

    const el = screen.getByTestId('floating-window-map');
    expect(el.style.left).toBe('100px');
    expect(el.style.top).toBe('100px');
    expect(el.style.width).toBe('480px');
    expect(el.style.height).toBe('360px');
    expect(el.style.zIndex).toBe('100');
  });

  it('renders panel label in title bar', () => {
    render(
      <FloatingWindow {...defaults}>
        <div>content</div>
      </FloatingWindow>,
    );

    expect(screen.getByText('Map')).toBeTruthy();
  });

  it('renders children content', () => {
    render(
      <FloatingWindow {...defaults}>
        <div>test content</div>
      </FloatingWindow>,
    );

    expect(screen.getByText('test content')).toBeTruthy();
  });

  it('calls onClose when close button clicked', () => {
    const onClose = vi.fn();
    render(
      <FloatingWindow {...defaults} onClose={onClose}>
        <div>content</div>
      </FloatingWindow>,
    );

    fireEvent.click(screen.getByTestId('floating-close-map'));
    expect(onClose).toHaveBeenCalledWith('float-1');
  });

  it('calls onDock when dock button clicked', () => {
    const onDock = vi.fn();
    render(
      <FloatingWindow {...defaults} onDock={onDock}>
        <div>content</div>
      </FloatingWindow>,
    );

    fireEvent.click(screen.getByText('dock'));
    expect(onDock).toHaveBeenCalledWith('float-1', 'map');
  });

  it('calls onFocus when window clicked', () => {
    const onFocus = vi.fn();
    render(
      <FloatingWindow {...defaults} onFocus={onFocus}>
        <div>content</div>
      </FloatingWindow>,
    );

    fireEvent.pointerDown(screen.getByTestId('floating-window-map'));
    expect(onFocus).toHaveBeenCalledWith('float-1');
  });

  it('has resize handle', () => {
    render(
      <FloatingWindow {...defaults}>
        <div>content</div>
      </FloatingWindow>,
    );

    expect(screen.getByTestId('floating-resize-map')).toBeTruthy();
  });

  it('has draggable title bar', () => {
    render(
      <FloatingWindow {...defaults}>
        <div>content</div>
      </FloatingWindow>,
    );

    expect(screen.getByTestId('floating-titlebar-map')).toBeTruthy();
  });
});
