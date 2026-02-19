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
    <div className="overflow-y-auto flex-1">
      <div className="flex flex-wrap gap-1.5 mb-2.5 text-[11px] text-[#7a9cc8]">
        <span className="text-[#4a6a9a]">Data pool: </span>
        {Object.entries(research.data_pool).map(([kind, amount]) => (
          <span key={kind}>
            {kind}: {amount.toFixed(1)}
          </span>
        ))}
        {Object.keys(research.data_pool).length === 0 && (
          <span className="text-[#3a5070]">—</span>
        )}
      </div>
      <div>
        {[...allTechIds].map((techId) => {
          const evidence = research.evidence[techId] ?? 0
          const isUnlocked = research.unlocked.includes(techId)
          const prob = unlockProbability(evidence)
          return (
            <div key={techId} className="py-1.5 border-b border-[#0d1226] text-[11px]">
              <div className="text-[#70a0d0] mb-0.5">{techId}</div>
              <div className="text-[#506080]">evidence: {evidence.toFixed(1)}</div>
              {isUnlocked ? (
                <div className="text-[#4caf7d] mt-0.5">✓ unlocked</div>
              ) : (
                <div className="text-[#506080] mt-0.5">p(unlock): {(prob * 100).toFixed(1)}%</div>
              )}
            </div>
          )
        })}
        {allTechIds.size === 0 && <div className="text-[#3a5070] italic">no research data yet</div>}
      </div>
    </div>
  )
}
