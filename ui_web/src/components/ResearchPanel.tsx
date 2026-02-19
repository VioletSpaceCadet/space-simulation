import type { ResearchState } from '../types'

interface Props {
  research: ResearchState
}

function unlockProbability(evidence: number, difficulty = 200): number {
  return 1 - Math.exp(-evidence / difficulty)
}

export function ResearchPanel({ research }: Props) {
  const allTechIds = new Set([...research.unlocked, ...Object.keys(research.evidence)])

  return (
    <div className="research-panel">
      <div className="data-pool">
        <span className="label">Data pool: </span>
        {Object.entries(research.data_pool).map(([kind, amount]) => (
          <span key={kind} className="data-item">
            {kind}: {amount.toFixed(1)}
          </span>
        ))}
        {Object.keys(research.data_pool).length === 0 && (
          <span className="data-item empty">—</span>
        )}
      </div>
      <div className="tech-list">
        {[...allTechIds].map((techId) => {
          const evidence = research.evidence[techId] ?? 0
          const isUnlocked = research.unlocked.includes(techId)
          const prob = unlockProbability(evidence)
          return (
            <div key={techId} className={`tech-row ${isUnlocked ? 'tech-unlocked' : ''}`}>
              <div className="tech-id">{techId}</div>
              <div className="tech-evidence">evidence: {evidence.toFixed(1)}</div>
              {isUnlocked ? (
                <div className="tech-status unlocked">✓ unlocked</div>
              ) : (
                <div className="tech-status">p(unlock): {(prob * 100).toFixed(1)}%</div>
              )}
            </div>
          )
        })}
        {allTechIds.size === 0 && <div className="tech-empty">no research data yet</div>}
      </div>
    </div>
  )
}
