import type { AsteroidState } from '../types'

interface Props {
  asteroids: Record<string, AsteroidState>
}

function pct(value: number): string {
  return `${Math.round(value * 100)}%`
}

function compositionSummary(composition: Record<string, number> | null): string {
  if (!composition) return '—'
  return Object.entries(composition)
    .sort(([, a], [, b]) => b - a)
    .map(([el, frac]) => `${el} ${pct(frac)}`)
    .join(' | ')
}

function tagSummary(tagBeliefs: [string, number][]): string {
  if (tagBeliefs.length === 0) return '—'
  return tagBeliefs.map(([tag, conf]) => `${tag} (${pct(conf)})`).join(', ')
}

export function AsteroidTable({ asteroids }: Props) {
  const rows = Object.values(asteroids)

  if (rows.length === 0) {
    return (
      <div className="overflow-auto flex-1">
        <div className="text-faint italic">no bodies discovered</div>
      </div>
    )
  }

  return (
    <div className="overflow-auto flex-1">
      <table className="min-w-max w-full border-collapse text-[11px]">
        <thead>
          <tr>
            <th className="text-left text-label px-2 py-1 border-b border-edge font-normal">ID</th>
            <th className="text-left text-label px-2 py-1 border-b border-edge font-normal">Node</th>
            <th className="text-left text-label px-2 py-1 border-b border-edge font-normal">Tags</th>
            <th className="text-left text-label px-2 py-1 border-b border-edge font-normal">Composition</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((asteroid) => (
            <tr key={asteroid.id}>
              <td className="px-2 py-0.5 border-b border-surface">{asteroid.id}</td>
              <td className="px-2 py-0.5 border-b border-surface">{asteroid.location_node}</td>
              <td className="px-2 py-0.5 border-b border-surface">{tagSummary(asteroid.knowledge.tag_beliefs)}</td>
              <td className="px-2 py-0.5 border-b border-surface">{compositionSummary(asteroid.knowledge.composition)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
