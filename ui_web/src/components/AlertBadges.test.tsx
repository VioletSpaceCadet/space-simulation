import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';

import type { ActiveAlert } from '../types';

import { AlertBadges } from './AlertBadges';

function makeAlert(id: string, severity: 'Warning' | 'Critical' = 'Warning'): ActiveAlert {
  return {
    alert_id: id,
    severity,
    message: `Message for ${id}`,
    suggested_action: `Action for ${id}`,
    tick: 100,
  };
}

describe('AlertBadges', () => {
  it('renders nothing when no alerts', () => {
    const { container } = render(
      <AlertBadges alerts={new Map()} dismissed={new Set()} onDismiss={() => {}} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders nothing when all alerts dismissed', () => {
    const alerts = new Map([
      ['low_fuel', makeAlert('low_fuel')],
      ['high_wear', makeAlert('high_wear')],
    ]);
    const dismissed = new Set(['low_fuel', 'high_wear']);
    const { container } = render(
      <AlertBadges alerts={alerts} dismissed={dismissed} onDismiss={() => {}} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('renders alert badges for visible alerts', () => {
    const alerts = new Map([
      ['low_fuel', makeAlert('low_fuel')],
      ['high_wear', makeAlert('high_wear')],
    ]);
    render(
      <AlertBadges alerts={alerts} dismissed={new Set()} onDismiss={() => {}} />,
    );
    expect(screen.getByText('low fuel')).toBeInTheDocument();
    expect(screen.getByText('high wear')).toBeInTheDocument();
  });

  it('calls onDismiss when dismiss button clicked', () => {
    const onDismiss = vi.fn();
    const alerts = new Map([['low_fuel', makeAlert('low_fuel')]]);
    render(
      <AlertBadges alerts={alerts} dismissed={new Set()} onDismiss={onDismiss} />,
    );
    const dismissButton = screen.getByText('×');
    fireEvent.click(dismissButton);
    expect(onDismiss).toHaveBeenCalledWith('low_fuel');
  });

  it('toggles expanded state on click', () => {
    const alerts = new Map([['low_fuel', makeAlert('low_fuel')]]);
    render(
      <AlertBadges alerts={alerts} dismissed={new Set()} onDismiss={() => {}} />,
    );
    // Not expanded initially
    expect(screen.queryByText('Message for low_fuel')).not.toBeInTheDocument();

    // Click badge to expand
    fireEvent.click(screen.getByText('low fuel'));
    expect(screen.getByText('Message for low_fuel')).toBeInTheDocument();
    expect(screen.getByText('Action for low_fuel')).toBeInTheDocument();

    // Click again to collapse
    fireEvent.click(screen.getByText('low fuel'));
    expect(screen.queryByText('Message for low_fuel')).not.toBeInTheDocument();
  });

  it('renders thermal alerts with readable labels', () => {
    const alerts = new Map([
      ['OVERHEAT_WARNING', makeAlert('OVERHEAT_WARNING')],
      ['OVERHEAT_CRITICAL', makeAlert('OVERHEAT_CRITICAL', 'Critical')],
    ]);
    render(
      <AlertBadges alerts={alerts} dismissed={new Set()} onDismiss={() => {}} />,
    );
    expect(screen.getByText('Overheat Warning')).toBeInTheDocument();
    expect(screen.getByText('Overheat Critical')).toBeInTheDocument();
  });

  it('navigates to fleet panel when thermal alert clicked', () => {
    const onNavigate = vi.fn();
    const alerts = new Map([
      ['OVERHEAT_WARNING', makeAlert('OVERHEAT_WARNING')],
    ]);
    render(
      <AlertBadges
        alerts={alerts}
        dismissed={new Set()}
        onDismiss={() => {}}
        onNavigateToPanel={onNavigate}
      />,
    );
    fireEvent.click(screen.getByText('Overheat Warning'));
    expect(onNavigate).toHaveBeenCalledWith('fleet');
  });

  it('does not navigate when non-thermal alert clicked', () => {
    const onNavigate = vi.fn();
    const alerts = new Map([['low_fuel', makeAlert('low_fuel')]]);
    render(
      <AlertBadges
        alerts={alerts}
        dismissed={new Set()}
        onDismiss={() => {}}
        onNavigateToPanel={onNavigate}
      />,
    );
    fireEvent.click(screen.getByText('low fuel'));
    expect(onNavigate).not.toHaveBeenCalled();
  });

  it('navigates to fleet panel for OVERHEAT_CRITICAL', () => {
    const onNavigate = vi.fn();
    const alerts = new Map([
      ['OVERHEAT_CRITICAL', makeAlert('OVERHEAT_CRITICAL', 'Critical')],
    ]);
    render(
      <AlertBadges
        alerts={alerts}
        dismissed={new Set()}
        onDismiss={() => {}}
        onNavigateToPanel={onNavigate}
      />,
    );
    fireEvent.click(screen.getByText('Overheat Critical'));
    expect(onNavigate).toHaveBeenCalledWith('fleet');
  });
});
