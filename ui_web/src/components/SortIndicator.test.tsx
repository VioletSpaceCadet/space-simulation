import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';

import { SortIndicator } from './SortIndicator';

describe('SortIndicator', () => {
  it('shows neutral indicator when sortConfig is null', () => {
    render(<SortIndicator column="name" sortConfig={null} />);
    expect(screen.getByText('⇅')).toBeInTheDocument();
  });

  it('shows neutral indicator when column does not match', () => {
    render(<SortIndicator column="name" sortConfig={{ key: 'cargo', direction: 'asc' }} />);
    expect(screen.getByText('⇅')).toBeInTheDocument();
  });

  it('shows up arrow for ascending', () => {
    render(<SortIndicator column="name" sortConfig={{ key: 'name', direction: 'asc' }} />);
    expect(screen.getByText('▲')).toBeInTheDocument();
  });

  it('shows down arrow for descending', () => {
    render(<SortIndicator column="name" sortConfig={{ key: 'name', direction: 'desc' }} />);
    expect(screen.getByText('▼')).toBeInTheDocument();
  });
});
