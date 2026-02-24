import type { DomainProgress, ResearchState } from '../types'

interface Props {
  research: ResearchState
}

function totalEvidence(dp: DomainProgress | undefined): number {
  if (!dp) return 0
  return Object.values(dp.points).reduce((sum, v) => sum + v, 0)
}

function unlockProbability(evidence: number, difficulty = 200): number {
  return 1 - Math.exp(-evidence / difficulty)
}

export function ResearchPanel({ research }: Props) {
  const allTechIds = new Set([...research.unlocked, ...Object.keys(research.evidence)])

  return (
    <div className="overflow-y-auto flex-1">
      <div className="flex flex-wrap gap-1.5 mb-2.5 text-[11px] text-dim">
        <span className="text-label">Data pool: </span>
        {Object.entries(research.data_pool).map(([kind, amount]) => (
          <span key={kind}>
            {kind}: {amount.toFixed(1)}
          </span>
        ))}
        {Object.keys(research.data_pool).length === 0 && (
          <span className="text-faint">—</span>
        )}
      </div>
      <div>
        {[...allTechIds].map((techId) => {
          const evidence = totalEvidence(research.evidence[techId])
          const isUnlocked = research.unlocked.includes(techId)
          const prob = unlockProbability(evidence)
          return (
            <div key={techId} className="py-1.5 border-b border-surface text-[11px]">
              <div className="text-accent mb-0.5">{techId}</div>
              <div className="text-muted">evidence: {evidence.toFixed(1)}</div>
              {isUnlocked ? (
                <div className="text-online mt-0.5">✓ unlocked</div>
              ) : (
                <div className="text-muted mt-0.5">p(unlock): {(prob * 100).toFixed(1)}%</div>
              )}
            </div>
          )
        })}
        {allTechIds.size === 0 && <div className="text-faint italic">no research data yet</div>}
      </div>
    </div>
  )
}
