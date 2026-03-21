import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import type { LabRateInfo } from '../types';

import { LabStatusSection } from './LabStatusSection';

const techNames: Record<string, string> = {
  tech_a: 'Alpha',
  tech_b: 'Beta',
};

const assignedLab: LabRateInfo = {
  station_id: 's1',
  module_id: 'm1',
  module_name: 'Survey Lab',
  assigned_tech: 'tech_a',
  domain: 'Survey',
  points_per_hour: 4.0,
  starved: false,
  enabled: true,
};

const idleLab: LabRateInfo = {
  station_id: 's1',
  module_id: 'm2',
  module_name: 'Materials Lab',
  assigned_tech: null,
  domain: 'Materials',
  points_per_hour: 0,
  starved: false,
  enabled: true,
};

const starvedLab: LabRateInfo = {
  station_id: 's1',
  module_id: 'm3',
  module_name: 'Mfg Lab',
  assigned_tech: 'tech_b',
  domain: 'Manufacturing',
  points_per_hour: 2.5,
  starved: true,
  enabled: true,
};

describe('LabStatusSection', () => {
  it('renders lab name and rate', () => {
    render(<LabStatusSection labs={[assignedLab]} techNames={techNames} />);
    expect(screen.getByText('Survey Lab')).toBeInTheDocument();
    expect(screen.getByText('+4.0/hr')).toBeInTheDocument();
  });

  it('shows idle badge for unassigned lab', () => {
    render(<LabStatusSection labs={[idleLab]} techNames={techNames} />);
    expect(screen.getByText('idle')).toBeInTheDocument();
  });

  it('shows active badge for assigned non-starved lab', () => {
    render(<LabStatusSection labs={[assignedLab]} techNames={techNames} />);
    expect(screen.getByText('active')).toBeInTheDocument();
  });

  it('shows starved badge for starved lab', () => {
    render(<LabStatusSection labs={[starvedLab]} techNames={techNames} />);
    expect(screen.getByText('starved')).toBeInTheDocument();
  });

  it('renders empty state when no labs', () => {
    render(<LabStatusSection labs={[]} techNames={techNames} />);
    expect(screen.getByText(/no labs/i)).toBeInTheDocument();
  });
});
