import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';

import { ErrorBoundary } from './ErrorBoundary';

function ThrowingComponent({ shouldThrow }: { shouldThrow: boolean }) {
  if (shouldThrow) {
    throw new Error('Test render error');
  }
  return <div>Normal content</div>;
}

describe('ErrorBoundary', () => {
  it('renders children when no error', () => {
    render(
      <ErrorBoundary panelName="Test Panel">
        <div>Child content</div>
      </ErrorBoundary>,
    );
    expect(screen.getByText('Child content')).toBeInTheDocument();
  });

  it('shows fallback UI when child throws during render', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    render(
      <ErrorBoundary panelName="Fleet">
        <ThrowingComponent shouldThrow />
      </ErrorBoundary>,
    );

    expect(screen.getByText('Fleet encountered an error')).toBeInTheDocument();
    expect(screen.getByText('Test render error')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /retry/i })).toBeInTheDocument();

    consoleSpy.mockRestore();
  });

  it('logs error to console with panel name', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    render(
      <ErrorBoundary panelName="Research">
        <ThrowingComponent shouldThrow />
      </ErrorBoundary>,
    );

    expect(consoleSpy).toHaveBeenCalledWith(
      '[ErrorBoundary] Research crashed:',
      expect.any(Error),
      expect.any(String),
    );

    consoleSpy.mockRestore();
  });

  it('recovers when retry is clicked and child no longer throws', async () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});
    const user = userEvent.setup();

    const { rerender } = render(
      <ErrorBoundary panelName="Map">
        <ThrowingComponent shouldThrow />
      </ErrorBoundary>,
    );

    expect(screen.getByText('Map encountered an error')).toBeInTheDocument();

    // Re-render with non-throwing child before clicking retry
    rerender(
      <ErrorBoundary panelName="Map">
        <ThrowingComponent shouldThrow={false} />
      </ErrorBoundary>,
    );

    await user.click(screen.getByRole('button', { name: /retry/i }));

    expect(screen.getByText('Normal content')).toBeInTheDocument();

    consoleSpy.mockRestore();
  });

  it('does not crash other panels when one panel throws', () => {
    const consoleSpy = vi.spyOn(console, 'error').mockImplementation(() => {});

    render(
      <div>
        <ErrorBoundary panelName="Good Panel">
          <div>Good panel content</div>
        </ErrorBoundary>
        <ErrorBoundary panelName="Bad Panel">
          <ThrowingComponent shouldThrow />
        </ErrorBoundary>
      </div>,
    );

    expect(screen.getByText('Good panel content')).toBeInTheDocument();
    expect(screen.getByText('Bad Panel encountered an error')).toBeInTheDocument();

    consoleSpy.mockRestore();
  });
});
