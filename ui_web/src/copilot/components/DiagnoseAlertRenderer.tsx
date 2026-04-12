/**
 * Render component for the `diagnose_alert` tool call.
 * Renders an AlertDetailCard with the diagnosis result.
 */

import { ToolCallStatus } from '@copilotkit/core';

import { AlertDetailCard } from './AlertDetail';

type DiagnoseArgs = { alert_id: string };

type DiagnoseData = Parameters<typeof AlertDetailCard>[0]['data'];

type RenderProps =
  | { args: Partial<DiagnoseArgs>; status: ToolCallStatus.InProgress; result: undefined }
  | { args: DiagnoseArgs; status: ToolCallStatus.Executing; result: undefined }
  | { args: DiagnoseArgs; status: ToolCallStatus.Complete; result: string };

/** Parse diagnosis result JSON, returning null on failure. */
function parseDiagnoseResult(result: string): DiagnoseData | null {
  try {
    return JSON.parse(result) as DiagnoseData;
  } catch {
    return null;
  }
}

export function DiagnoseAlertRenderer(props: RenderProps) {
  if (props.status !== ToolCallStatus.Complete) {
    return (
      <div style={{
        padding: '8px',
        fontSize: '12px',
        color: '#8a8e98',
        display: 'flex',
        alignItems: 'center',
        gap: '6px',
      }}>
        <span style={{
          display: 'inline-block',
          width: '12px',
          height: '12px',
          border: '2px solid #8a8e9840',
          borderTopColor: '#8a8e98',
          borderRadius: '50%',
          animation: 'cpk-spin 0.8s linear infinite',
        }} />
        Diagnosing alert {props.args.alert_id ?? ''}...
      </div>
    );
  }

  const data = parseDiagnoseResult(props.result);
  if (data === null) {
    return <div style={{ fontSize: '12px', color: '#e05252' }}>Failed to parse diagnosis result.</div>;
  }

  return <AlertDetailCard data={data} />;
}
