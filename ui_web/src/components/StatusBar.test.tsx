import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { StatusBar } from './StatusBar';

const defaultProps = {
  paused: false,
  onTogglePause: () => {},
  alerts: new Map(),
  dismissedAlerts: new Set<string>(),
  onDismissAlert: () => {},
  minutesPerTick: 1,
  activeSpeed: 10,
  onSetSpeed: () => {},
};

describe('StatusBar', () => {
  it('renders tick number', () => {
    render(<StatusBar tick={1440} connected measuredTickRate={10} {...defaultProps} />);
    expect(screen.getByText(/1440/)).toBeInTheDocument();
  });

  it('shows day and hour derived from tick', () => {
    render(<StatusBar tick={1440} connected measuredTickRate={10} {...defaultProps} />);
    expect(screen.getByText(/day 1/i)).toBeInTheDocument();
  });

  it('shows connected when connected', () => {
    render(<StatusBar tick={0} connected measuredTickRate={10} {...defaultProps} />);
    expect(screen.getByText(/connected/i)).toBeInTheDocument();
  });

  it('shows reconnecting when not connected', () => {
    render(<StatusBar tick={0} connected={false} measuredTickRate={10} {...defaultProps} />);
    expect(screen.getByText(/reconnecting/i)).toBeInTheDocument();
  });

  it('displays measured tick rate', () => {
    render(<StatusBar tick={0} connected measuredTickRate={9.7} {...defaultProps} />);
    expect(screen.getByText(/~9\.7 t\/s/)).toBeInTheDocument();
  });

  it('floors fractional ticks for display', () => {
    render(<StatusBar tick={1440.7} connected measuredTickRate={10} {...defaultProps} />);
    expect(screen.getByText(/1440/)).toBeInTheDocument();
    // Should not show the fractional part
    expect(screen.queryByText(/1440\.7/)).not.toBeInTheDocument();
  });

  it('renders a save button', () => {
    render(<StatusBar tick={0} connected measuredTickRate={10} {...defaultProps} />);
    expect(screen.getByRole('button', { name: /save/i })).toBeInTheDocument();
  });

  it('renders Running when not paused', () => {
    render(<StatusBar tick={0} connected measuredTickRate={10} {...defaultProps} paused={false} />);
    expect(screen.getByRole('button', { name: /running/i })).toBeInTheDocument();
  });

  it('renders Paused when paused', () => {
    render(<StatusBar tick={0} connected measuredTickRate={10} {...defaultProps} paused />);
    expect(screen.getByRole('button', { name: /paused/i })).toBeInTheDocument();
  });

  it('shows correct day with minutesPerTick=60', () => {
    render(<StatusBar {...defaultProps} tick={24} connected measuredTickRate={10} minutesPerTick={60} />);
    expect(screen.getByText(/day 1/i)).toBeInTheDocument();
  });

  it('calls onTogglePause when pause button is clicked', async () => {
    const onTogglePause = vi.fn();
    render(<StatusBar tick={0} connected measuredTickRate={10} {...defaultProps} onTogglePause={onTogglePause} />);
    await userEvent.click(screen.getByRole('button', { name: /running/i }));
    expect(onTogglePause).toHaveBeenCalledOnce();
  });
});
