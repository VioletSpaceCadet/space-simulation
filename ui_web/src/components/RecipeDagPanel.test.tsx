import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { SimSnapshot } from '../types';

import { RecipeDagPanel } from './RecipeDagPanel';

vi.mock('../hooks/useContent', () => ({
  useContent: () => ({
    content: {
      techs: [],
      lab_rates: [],
      data_rates: {},
      minutes_per_tick: 60,
      recipes: {
        smelt_fe: {
          id: 'smelt_fe',
          inputs: [{ filter: { Element: 'Fe' }, amount: { Kg: 10 } }],
          outputs: [{ Material: { element: 'Fe' } }],
          efficiency: 1,
          required_tech: null,
        },
      },
    },
    refetch: vi.fn(),
  }),
}));

function makeSnapshot(): SimSnapshot {
  return {
    meta: {
      tick: 100,
      seed: 1,
      content_version: '1',
      ticks_per_sec: 10,
      paused: false,
      minutes_per_tick: 60,
      trade_unlock_tick: 1000,
    },
    balance: 1_000_000,
    scan_sites: [],
    asteroids: {},
    ships: {},
    stations: {
      station_1: {
        id: 'station_1',
        position: { parent_body: 'sun', radius_au_um: 1000000, angle_mdeg: 0 },
        power_available_per_tick: 100,
        inventory: [
          { kind: 'Material', element: 'Fe', kg: 500, quality: 0.9 },
        ],
        cargo_capacity_m3: 1000,
        modules: [
          {
            id: 'proc_1',
            def_id: 'smelter',
            enabled: true,
            kind_state: { Processor: { threshold_kg: 10, ticks_since_last_run: 0, stalled: false, selected_recipe: 'smelt_fe' } },
            wear: { wear: 0 },
          },
        ],
        power: {
          generated_kw: 100, consumed_kw: 50, deficit_kw: 0,
          battery_discharge_kw: 0, battery_charge_kw: 0, battery_stored_kwh: 0,
        },
      },
    },
    research: { unlocked: [], data_pool: {}, evidence: {}, action_counts: {} },
    body_absolutes: {},
  };
}

describe('RecipeDagPanel', () => {
  it('renders loading state when snapshot is null', () => {
    render(<RecipeDagPanel snapshot={null} events={[]} currentTick={0} />);
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('renders with mock data showing graph nodes', () => {
    const snapshot = makeSnapshot();
    render(<RecipeDagPanel snapshot={snapshot} events={[]} currentTick={100} />);
    // The recipe node should appear
    expect(screen.getByTestId('recipe-node-smelt_fe')).toBeInTheDocument();
    // The item node for Fe should appear
    expect(screen.getByTestId('item-node-Fe')).toBeInTheDocument();
  });

  it('toggles filter between all and active', () => {
    const snapshot = makeSnapshot();
    render(<RecipeDagPanel snapshot={snapshot} events={[]} currentTick={100} />);
    const filterButton = screen.getByRole('button', { name: /all/i });
    expect(filterButton).toBeInTheDocument();
    fireEvent.click(filterButton);
    expect(screen.getByRole('button', { name: /active/i })).toBeInTheDocument();
  });
});
