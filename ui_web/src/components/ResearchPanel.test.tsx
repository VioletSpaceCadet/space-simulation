import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { ResearchState } from '../types';

import { ResearchPanel } from './ResearchPanel';

vi.mock('../hooks/useContent', () => ({
  useContent: () => ({
    content: {
      techs: [{ id: 'tech_a', name: 'Alpha', prereqs: [], domain_requirements: { Survey: 100 }, accepted_data: [], difficulty: 200, effects: [] }],
      lab_rates: [{ station_id: 's1', module_id: 'm1', module_name: 'Survey Lab', assigned_tech: 'tech_a', domain: 'Survey', points_per_hour: 4.0, starved: false, enabled: true }],
      data_rates: { SurveyData: 1.2 },
      minutes_per_tick: 60,
    },
    refetch: vi.fn(),
  }),
}));

const research: ResearchState = {
  unlocked: [],
  data_pool: { SurveyData: 42.5 },
  evidence: { tech_a: { points: { Survey: 50 } } },
  action_counts: {},
};

describe('ResearchPanel', () => {
  it('renders data pool section', () => {
    render(<ResearchPanel research={research} />);
    // "Survey" appears in DataPoolSection and in TechTreeDAG domain bars; check count
    expect(screen.getAllByText(/Survey/).length).toBeGreaterThan(0);
    expect(screen.getByText(/42\.5/)).toBeInTheDocument();
  });

  it('renders lab status section', () => {
    render(<ResearchPanel research={research} />);
    expect(screen.getByText('Survey Lab')).toBeInTheDocument();
  });

  it('renders tech tree with tech name', () => {
    render(<ResearchPanel research={research} />);
    // Alpha appears in both TechTreeDAG and LabStatusSection (as assigned tech name)
    expect(screen.getAllByText('Alpha').length).toBeGreaterThan(0);
  });
});
