import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { SimEvent } from '../types';

import { EventsFeed } from './EventsFeed';

const events: SimEvent[] = [
  { id: 'evt_000001', tick: 10, event: { TechUnlocked: { tech_id: 'tech_deep_scan_v1' } } },
  { id: 'evt_000002', tick: 5, event: { AsteroidDiscovered: { asteroid_id: 'asteroid_0001' } } },
];

describe('EventsFeed', () => {
  it('renders event IDs', () => {
    render(<EventsFeed events={events} />);
    expect(screen.getByText(/evt_000001/)).toBeInTheDocument();
  });

  it('renders event type name', () => {
    render(<EventsFeed events={events} />);
    expect(screen.getByText(/TechUnlocked/)).toBeInTheDocument();
  });

  it('shows empty state with no events', () => {
    render(<EventsFeed events={[]} />);
    expect(screen.getByText(/waiting for stream data/i)).toBeInTheDocument();
  });
});
