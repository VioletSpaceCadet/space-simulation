import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import { PanelHeader } from './PanelHeader';

describe('PanelHeader', () => {
  it('renders the title', () => {
    render(<PanelHeader title="Events" collapsed={false} onToggle={() => {}} />);
    expect(screen.getByText('Events')).toBeInTheDocument();
  });

  it('shows expanded indicator when not collapsed', () => {
    render(<PanelHeader title="Events" collapsed={false} onToggle={() => {}} />);
    expect(screen.getByText('▾')).toBeInTheDocument();
  });

  it('shows collapsed indicator when collapsed', () => {
    render(<PanelHeader title="Events" collapsed onToggle={() => {}} />);
    expect(screen.getByText('▸')).toBeInTheDocument();
  });

  it('calls onToggle when clicked', () => {
    const onToggle = vi.fn();
    render(<PanelHeader title="Events" collapsed={false} onToggle={onToggle} />);
    fireEvent.click(screen.getByText('Events'));
    expect(onToggle).toHaveBeenCalledOnce();
  });
});
