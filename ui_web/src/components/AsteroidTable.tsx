import { useSortableData } from '../hooks/useSortableData';
import type { AsteroidState } from '../types';

import { SortIndicator } from './SortIndicator';

interface Props {
  asteroids: Record<string, AsteroidState>
}

function pct(value: number): string {
  return `${Math.round(value * 100)}%`;
}

function compositionSummary(composition: Record<string, number> | null): string {
  if (!composition) {return '—';}
  return Object.entries(composition)
    .sort(([, a], [, b]) => b - a)
    .map(([el, frac]) => `${el} ${pct(frac)}`)
    .join(' | ');
}

const TAG_COLORS: Record<string, { bg: string; text: string }> = {
  IronRich: { bg: 'rgba(196, 112, 56, 0.15)', text: '#c47038' },
  VolatileRich: { bg: 'rgba(56, 160, 196, 0.15)', text: '#38a0c4' },
  Carbonaceous: { bg: 'rgba(180, 140, 60, 0.15)', text: '#b48c3c' },
};

function TagBadge({ tag, confidence }: { tag: string; confidence: number }) {
  const colors = TAG_COLORS[tag] ?? { bg: 'rgba(138, 142, 152, 0.15)', text: '#8a8e98' };
  return (
    <span
      style={{ background: colors.bg, color: colors.text, padding: '0 4px', borderRadius: 2, fontSize: 10 }}
    >
      {tag} {pct(confidence)}
    </span>
  );
}

function TagBadges({ tagBeliefs }: { tagBeliefs: [string, number][] }) {
  if (tagBeliefs.length === 0) {return <span className="text-faint">—</span>;}
  return (
    <span style={{ display: 'flex', gap: 3, flexWrap: 'wrap' }}>
      {tagBeliefs.map(([tag, conf]) => <TagBadge key={tag} tag={tag} confidence={conf} />)}
    </span>
  );
}

function primaryFraction(asteroid: AsteroidState): number {
  const comp = asteroid.knowledge.composition;
  if (!comp) {return 0;}
  return Math.max(...Object.values(comp), 0);
}

interface SortableAsteroid {
  id: string
  parent_body: string
  mass_kg: number
  primary_fraction: number
  asteroid: AsteroidState
}

export function AsteroidTable({ asteroids }: Props) {
  const rows = Object.values(asteroids);

  const sortableRows: SortableAsteroid[] = rows.map((asteroid) => ({
    id: asteroid.id,
    parent_body: asteroid.position.parent_body,
    mass_kg: asteroid.mass_kg ?? -1,
    primary_fraction: primaryFraction(asteroid),
    asteroid,
  }));

  const { sortedData, sortConfig, requestSort } = useSortableData(sortableRows);

  if (rows.length === 0) {
    return (
      <div className="overflow-auto flex-1">
        <div className="text-faint italic">no bodies discovered</div>
      </div>
    );
  }

  const headerClass = 'text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none';

  return (
    <div className="overflow-auto flex-1">
      <table className="min-w-max w-full border-collapse text-[11px]">
        <thead>
          <tr>
            <th className={headerClass} onClick={() => requestSort('id')}>
              ID<SortIndicator column="id" sortConfig={sortConfig} />
            </th>
            <th className={headerClass} onClick={() => requestSort('parent_body')}>
              Body<SortIndicator column="parent_body" sortConfig={sortConfig} />
            </th>
            <th className="text-left text-label px-2 py-1 border-b border-edge font-normal">Tags</th>
            <th className={headerClass} onClick={() => requestSort('primary_fraction')}>
              Composition<SortIndicator column="primary_fraction" sortConfig={sortConfig} />
            </th>
            <th className={headerClass} onClick={() => requestSort('mass_kg')}>
              Mass<SortIndicator column="mass_kg" sortConfig={sortConfig} />
            </th>
          </tr>
        </thead>
        <tbody>
          {sortedData.map(({ asteroid }) => (
            <tr key={asteroid.id}>
              <td className="px-2 py-0.5 border-b border-surface">{asteroid.id}</td>
              <td className="px-2 py-0.5 border-b border-surface">{asteroid.position.parent_body}</td>
              <td className="px-2 py-0.5 border-b border-surface"><TagBadges tagBeliefs={asteroid.knowledge.tag_beliefs} /></td>
              <td className="px-2 py-0.5 border-b border-surface text-cargo">{compositionSummary(asteroid.knowledge.composition)}</td>
              <td className="px-2 py-0.5 border-b border-surface">
                {asteroid.mass_kg === undefined
                  ? <span className="text-faint">—</span>
                  : asteroid.mass_kg > 0
                    ? <span className="text-bright">{asteroid.mass_kg.toLocaleString(undefined, { maximumFractionDigits: 0 })} kg</span>
                    : <span className="text-faint">depleted</span>}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
