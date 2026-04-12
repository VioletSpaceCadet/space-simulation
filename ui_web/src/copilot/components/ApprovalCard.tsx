/**
 * Shared approval card layout for CopilotKit `useHumanInTheLoop` actions.
 *
 * Renders a titled card with a description area, optional detail slots,
 * and approve/reject buttons. The buttons call the `respond` callback
 * from CopilotKit's Executing state to send the user's decision back
 * to the LLM.
 *
 * Used by every command-executing copilot action (pause, speed, command).
 */

import { useState } from 'react';

import { SEMANTIC_COLORS } from '../../config/theme';

export type ApprovalCardStatus = 'pending' | 'approved' | 'rejected' | 'error';

interface ApprovalCardProps {
  title: string;
  children?: React.ReactNode;
  onApprove: (() => void) | (() => Promise<void>);
  onReject: () => void;
  approveLabel?: string;
  rejectLabel?: string;
  approveDisabled?: boolean;
  status: ApprovalCardStatus;
}

const STATUS_LABELS: Record<ApprovalCardStatus, string> = {
  pending: '',
  approved: 'Approved',
  rejected: 'Cancelled',
  error: 'Error',
};

const STATUS_COLORS: Record<ApprovalCardStatus, string> = {
  pending: '',
  approved: SEMANTIC_COLORS.positive,
  rejected: SEMANTIC_COLORS.negative,
  error: SEMANTIC_COLORS.negative,
};

const STATUS_BORDER: Record<ApprovalCardStatus, React.CSSProperties> = {
  pending: {},
  approved: { borderColor: SEMANTIC_COLORS.positive },
  rejected: { borderColor: SEMANTIC_COLORS.negative, opacity: 0.7 },
  error: { borderColor: SEMANTIC_COLORS.negative },
};

export function ApprovalCard({
  title,
  children,
  onApprove,
  onReject,
  approveLabel = 'Approve',
  rejectLabel = 'Cancel',
  approveDisabled = false,
  status,
}: ApprovalCardProps) {
  const isDone = status !== 'pending';
  // Double-click guard: once the player clicks, disable further clicks
  // during the async gap before CopilotKit transitions to Complete.
  const [clicked, setClicked] = useState(false);

  return (
    <div style={{
      padding: '12px',
      borderRadius: '8px',
      backgroundColor: 'rgba(255,255,255,0.04)',
      border: '1px solid rgba(255,255,255,0.08)',
      ...STATUS_BORDER[status],
    }}>
      <div style={{
        fontSize: '13px',
        fontWeight: 600,
        color: 'var(--copilot-foreground, #e0e2e8)',
        marginBottom: children ? '8px' : '12px',
      }}>
        {title}
      </div>

      {children && (
        <div style={{ marginBottom: '12px' }}>
          {children}
        </div>
      )}

      {isDone ? (
        <div style={{
          fontSize: '11px',
          fontWeight: 600,
          textTransform: 'uppercase',
          letterSpacing: '0.05em',
          color: STATUS_COLORS[status],
        }}>
          {STATUS_LABELS[status]}
        </div>
      ) : (
        <div style={{ display: 'flex', gap: '8px' }}>
          <button
            type="button"
            disabled={clicked}
            onClick={() => {
              if (clicked) { return; }
              setClicked(true);
              onReject();
            }}
            style={{
              padding: '6px 14px',
              borderRadius: '6px',
              border: '1px solid rgba(255,255,255,0.12)',
              backgroundColor: 'transparent',
              color: 'var(--copilot-foreground, #a0a4b0)',
              fontSize: '12px',
              cursor: 'pointer',
            }}
          >
            {rejectLabel}
          </button>
          <button
            type="button"
            onClick={() => {
              if (clicked) { return; }
              setClicked(true);
              void onApprove();
            }}
            disabled={approveDisabled || clicked}
            style={{
              padding: '6px 14px',
              borderRadius: '6px',
              border: 'none',
              backgroundColor: approveDisabled ? '#3a3e48' : SEMANTIC_COLORS.positive,
              color: approveDisabled ? '#6b7080' : '#fff',
              fontSize: '12px',
              fontWeight: 600,
              cursor: approveDisabled ? 'not-allowed' : 'pointer',
            }}
          >
            {approveLabel}
          </button>
        </div>
      )}
    </div>
  );
}
