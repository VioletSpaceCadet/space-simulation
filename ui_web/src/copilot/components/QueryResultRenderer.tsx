/**
 * Dispatcher component for `query_game_state` tool renders.
 *
 * Receives the CopilotKit tool-call render props, parses the JSON
 * result, and routes to the appropriate visual component based on
 * `args.section`. During InProgress/Executing states shows a loading
 * indicator.
 */

import { ToolCallStatus } from '@copilotkit/core';

import type { QuerySection } from '../actions/queryHandlers';

import { AlertsSummaryCard } from './AlertDetail';
import { FleetTable } from './FleetTable';
import { StationSummary } from './StationSummary';

type QueryArgs = { section: QuerySection };

type RenderProps =
  | { args: Partial<QueryArgs>; status: ToolCallStatus.InProgress; result: undefined }
  | { args: QueryArgs; status: ToolCallStatus.Executing; result: undefined }
  | { args: QueryArgs; status: ToolCallStatus.Complete; result: string };

function LoadingIndicator({ section }: { section?: string }) {
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
      Querying {section ?? 'game state'}...
    </div>
  );
}

/** Parse the JSON result string, returning null on failure. */
function parseResult(result: string): unknown {
  try {
    return JSON.parse(result) as unknown;
  } catch {
    return null;
  }
}

/**
 * Renders a section-appropriate card for a `query_game_state`
 * tool result. Falls back to formatted JSON for sections without
 * a dedicated component (treasury, research, asteroids, summary).
 */
function SectionResult({ section, result }: { section: QuerySection; result: string }) {
  const data = parseResult(result);
  if (data === null) {
    return <div style={{ fontSize: '12px', color: '#e05252' }}>Failed to parse result.</div>;
  }

  switch (section) {
    case 'fleet':
      return <FleetTable data={data as Parameters<typeof FleetTable>[0]['data']} />;
    case 'stations':
      return <StationSummary data={data as Parameters<typeof StationSummary>[0]['data']} />;
    case 'alerts':
      return <AlertsSummaryCard data={data as Parameters<typeof AlertsSummaryCard>[0]['data']} />;
    default:
      // Treasury, research, asteroids, summary — render as formatted key-value pairs
      return <GenericResult section={section} data={data} />;
  }
}

/** Renders key-value pairs for sections without a dedicated component. */
function GenericResult({ section, data }: { section: string; data: unknown }) {
  if (typeof data !== 'object' || data === null) {
    return <div style={{ fontSize: '12px' }}>{String(data)}</div>;
  }

  const entries = Object.entries(data as Record<string, unknown>);
  return (
    <div style={{ padding: '8px 0' }}>
      <div style={{
        fontSize: '11px',
        fontWeight: 600,
        textTransform: 'uppercase',
        letterSpacing: '0.05em',
        color: 'var(--copilot-foreground, #c8ccd4)',
        marginBottom: '6px',
      }}>
        {section}
      </div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: '2px' }}>
        {entries.map(([key, value]) => (
          <div key={key} style={{
            display: 'flex',
            justifyContent: 'space-between',
            padding: '2px 8px',
            fontSize: '12px',
            borderRadius: '3px',
            backgroundColor: 'rgba(255,255,255,0.03)',
          }}>
            <span style={{ color: '#8a8e98' }}>{key.replace(/_/g, ' ')}</span>
            <span style={{ fontWeight: 500, fontVariantNumeric: 'tabular-nums' }}>
              {typeof value === 'object' ? JSON.stringify(value) : String(value)}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

export function QueryResultRenderer(props: RenderProps) {
  if (props.status !== ToolCallStatus.Complete) {
    return <LoadingIndicator section={props.args.section} />;
  }

  return <SectionResult section={props.args.section} result={props.result} />;
}
