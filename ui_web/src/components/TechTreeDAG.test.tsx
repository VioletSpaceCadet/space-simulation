import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { ResearchState, TechDef } from '../types';

import { TechTreeDAG } from './TechTreeDAG';

const techs: TechDef[] = [
  {
    id: 'a',
    name: 'Alpha Tech',
    prereqs: [],
    domain_requirements: { Survey: 100 },
    accepted_data: [],
    difficulty: 200,
    effects: [],
  },
  {
    id: 'b',
    name: 'Beta Tech',
    prereqs: ['a'],
    domain_requirements: { Materials: 50 },
    accepted_data: [],
    difficulty: 300,
    effects: [],
  },
];

const research: ResearchState = {
  unlocked: ['a'],
  data_pool: {},
  evidence: { b: { points: { Materials: 25 } } },
  action_counts: {},
};

describe('TechTreeDAG', () => {
  it('renders unlocked tech name', () => {
    render(<TechTreeDAG techs={techs} research={research} labAssignments={['b']} />);
    expect(screen.getByText('Alpha Tech')).toBeInTheDocument();
  });

  it('renders researching tech name', () => {
    render(<TechTreeDAG techs={techs} research={research} labAssignments={['b']} />);
    expect(screen.getByText('Beta Tech')).toBeInTheDocument();
  });

  it('renders empty state when no activity', () => {
    const emptyResearch: ResearchState = {
      unlocked: [],
      data_pool: {},
      evidence: {},
      action_counts: {},
    };
    render(<TechTreeDAG techs={techs} research={emptyResearch} labAssignments={[]} />);
    expect(screen.getByText(/no research activity/i)).toBeInTheDocument();
  });

  it('renders mystery nodes as ???', () => {
    // For a mystery node: need a locked node (c), then a child of locked (d = mystery)
    // a=unlocked, b=researching, c=locked (child of researching b, not lab-assigned),
    // d=mystery (child of locked c)
    const deepTechs: TechDef[] = [
      ...techs,
      {
        id: 'c',
        name: 'Gamma',
        prereqs: ['b'],
        domain_requirements: { Survey: 200 },
        accepted_data: [],
        difficulty: 100,
        effects: [],
      },
      {
        id: 'd',
        name: 'Delta',
        prereqs: ['c'],
        domain_requirements: {},
        accepted_data: [],
        difficulty: 100,
        effects: [],
      },
    ];
    render(<TechTreeDAG techs={deepTechs} research={research} labAssignments={['b']} />);
    expect(screen.getByText('???')).toBeInTheDocument();
  });

  it('renders domain progress bars', () => {
    render(<TechTreeDAG techs={techs} research={research} labAssignments={['b']} />);
    // Beta Tech has Materials domain requirement of 50, evidence of 25
    expect(screen.getByText('Materials')).toBeInTheDocument();
    expect(screen.getByText(/25/)).toBeInTheDocument();
    expect(screen.getByText(/50/)).toBeInTheDocument();
  });
});
