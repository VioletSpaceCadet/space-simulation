import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { SimEvent } from '../types';

import { EventsFeed } from './EventsFeed';

const events: SimEvent[] = [
  { id: 1, tick: 10, event: { TechUnlocked: { tech_id: 'tech_deep_scan_v1' } } },
  { id: 2, tick: 5, event: { AsteroidDiscovered: { asteroid_id: 'asteroid_0001' } } },
];

describe('EventsFeed', () => {
  it('renders event IDs', () => {
    render(<EventsFeed events={events} />);
    expect(screen.getByText('1')).toBeInTheDocument();
  });

  it('renders event type name', () => {
    render(<EventsFeed events={events} />);
    expect(screen.getByText(/TechUnlocked/)).toBeInTheDocument();
  });

  it('shows empty state with no events', () => {
    render(<EventsFeed events={[]} />);
    expect(screen.getByText(/waiting for stream data/i)).toBeInTheDocument();
  });

  it('renders SimEventFired with target and label', () => {
    const simEvents: SimEvent[] = [
      {
        id: 10, tick: 42, event: {
          SimEventFired: {
            event_def_id: 'evt_solar_flare',
            target: { type: 'station', station_id: 'station_earth_orbit' },
            effects_applied: [{ effect: { type: 'trigger_alert' }, target: { type: 'global' } }],
          },
        },
      },
    ];
    render(<EventsFeed events={simEvents} />);
    expect(screen.getByText('EVENT')).toBeInTheDocument();
    expect(screen.getByText('solar flare')).toBeInTheDocument();
    expect(screen.getByText(/station_earth_orbit/)).toBeInTheDocument();
  });

  it('renders SimEventExpired with effect ended label', () => {
    const simEvents: SimEvent[] = [
      {
        id: 20, tick: 100, event: {
          SimEventExpired: { event_def_id: 'evt_solar_flare' },
        },
      },
    ];
    render(<EventsFeed events={simEvents} />);
    expect(screen.getByText(/solar flare effect ended/)).toBeInTheDocument();
  });

  it('falls back to event_def_id when unknown event', () => {
    const simEvents: SimEvent[] = [
      {
        id: 30, tick: 50, event: {
          SimEventFired: {
            event_def_id: 'evt_unknown_thing',
            target: { type: 'global' },
            effects_applied: [],
          },
        },
      },
    ];
    render(<EventsFeed events={simEvents} />);
    expect(screen.getByText('unknown thing')).toBeInTheDocument();
  });
});
