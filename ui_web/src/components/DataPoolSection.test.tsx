import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { DataPoolSection } from './DataPoolSection';

describe('DataPoolSection', () => {
  it('renders data kind with amount and positive rate', () => {
    render(
      <DataPoolSection
        dataPool={{ SurveyData: 42.5 }}
        dataRates={{ SurveyData: 1.2 }}
      />,
    );
    expect(screen.getByText('Survey')).toBeInTheDocument();
    expect(screen.getByText('42.5')).toBeInTheDocument();
    expect(screen.getByText('+1.2/hr')).toBeInTheDocument();
    const rateEl = screen.getByText('+1.2/hr');
    expect(rateEl).toHaveStyle({ color: '#4caf7d' });
  });

  it('renders negative rate in red', () => {
    render(
      <DataPoolSection
        dataPool={{ AssayData: 10.0 }}
        dataRates={{ AssayData: -0.5 }}
      />,
    );
    expect(screen.getByText('-0.5/hr')).toBeInTheDocument();
    const rateEl = screen.getByText('-0.5/hr');
    expect(rateEl).toHaveStyle({ color: '#e05252' });
  });

  it('renders empty state when no data', () => {
    render(<DataPoolSection dataPool={{}} dataRates={{}} />);
    expect(screen.getByText(/no data/i)).toBeInTheDocument();
  });
});
