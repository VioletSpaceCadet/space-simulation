/**
 * Shared approval card layout for CopilotKit `useHumanInTheLoop` actions.
 *
 * Renders a titled card with a description area, optional detail slots,
 * and approve/reject buttons. The buttons call the `respond` callback
 * from CopilotKit's Executing state to send the user's decision back
 * to the LLM.
 *
 * Used by every command-executing copilot action (pause, speed, launch).
 */

import { SEMANTIC_COLORS } from '../../config/theme';

interface ApprovalCardProps {
  title: string;
  children?: React.ReactNode;
  onApprove: () => void;
  onReject: () => void;
  approveLabel?: string;
  rejectLabel?: string;
  approveDisabled?: boolean;
  status: 'pending' | 'approved' | 'rejected';
}

const STATUS_STYLES = {
  pending: {},
  approved: { borderColor: SEMANTIC_COLORS.positive },
  rejected: { borderColor: SEMANTIC_COLORS.negative, opacity: 0.7 },
} as const;

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

  return (
    <div style={{
      padding: '12px',
      borderRadius: '8px',
      backgroundColor: 'rgba(255,255,255,0.04)',
      border: '1px solid rgba(255,255,255,0.08)',
      ...STATUS_STYLES[status],
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
          color: status === 'approved' ? SEMANTIC_COLORS.positive : SEMANTIC_COLORS.negative,
        }}>
          {status === 'approved' ? 'Approved' : 'Cancelled'}
        </div>
      ) : (
        <div style={{ display: 'flex', gap: '8px' }}>
          <button
            type="button"
            onClick={onReject}
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
            onClick={onApprove}
            disabled={approveDisabled}
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
