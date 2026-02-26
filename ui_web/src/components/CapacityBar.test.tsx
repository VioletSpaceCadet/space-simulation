import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { CapacityBar } from './CapacityBar';

describe('CapacityBar', () => {
  it('renders percentage and kg values', () => {
    render(<CapacityBar usedKg={300} capacityKg={1000} />);
    expect(screen.getByText(/30%/)).toBeInTheDocument();
    expect(screen.getByText(/300/)).toBeInTheDocument();
  });

  it('renders progressbar with correct aria value', () => {
    render(<CapacityBar usedKg={750} capacityKg={1000} />);
    const bar = document.querySelector('[role="progressbar"]');
    expect(bar).toBeInTheDocument();
    expect(bar?.getAttribute('aria-valuenow')).toBe('75');
  });

  it('uses red color when above 90%', () => {
    const { container } = render(<CapacityBar usedKg={950} capacityKg={1000} />);
    const fill = container.querySelector('[data-testid="capacity-fill"]');
    expect(fill?.className).toMatch(/red/);
  });

  it('uses yellow color when above 70%', () => {
    const { container } = render(<CapacityBar usedKg={750} capacityKg={1000} />);
    const fill = container.querySelector('[data-testid="capacity-fill"]');
    expect(fill?.className).toMatch(/yellow/);
  });

  it('uses green color when below 70%', () => {
    const { container } = render(<CapacityBar usedKg={300} capacityKg={1000} />);
    const fill = container.querySelector('[data-testid="capacity-fill"]');
    expect(fill?.className).toMatch(/green/);
  });

  it('handles zero capacity without crashing', () => {
    render(<CapacityBar usedKg={0} capacityKg={0} />);
    expect(screen.getByText(/0%/)).toBeInTheDocument();
  });
});
