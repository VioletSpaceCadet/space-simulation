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
      <div className="asteroid-table">
        <div className="table-empty">no asteroids discovered</div>
      </div>
    )
  }

  return (
    <div className="asteroid-table">
      <table>
        <thead>
          <tr>
            <th>ID</th>
            <th>Node</th>
            <th>Tags</th>
            <th>Composition</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((asteroid) => (
            <tr key={asteroid.id}>
              <td>{asteroid.id}</td>
              <td>{asteroid.location_node}</td>
              <td>{tagSummary(asteroid.knowledge.tag_beliefs)}</td>
              <td>{compositionSummary(asteroid.knowledge.composition)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
