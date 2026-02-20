import { useRef } from 'react'
import { useSvgZoomPan } from '../hooks/useSvgZoomPan'
import type { OreCompositions } from '../hooks/useSimStream'
import type { SimSnapshot } from '../types'

interface Props {
  snapshot: SimSnapshot | null
  currentTick: number
  oreCompositions: OreCompositions
}

const RINGS: { nodeId: string; label: string; radius: number; isBelt: boolean }[] = [
  { nodeId: 'node_earth_orbit', label: 'Earth Orbit', radius: 100, isBelt: false },
  { nodeId: 'node_belt_inner', label: 'Inner Belt', radius: 200, isBelt: true },
  { nodeId: 'node_belt_mid', label: 'Mid Belt', radius: 300, isBelt: true },
  { nodeId: 'node_belt_outer', label: 'Outer Belt', radius: 400, isBelt: true },
]

export function SolarSystemMap({ snapshot, currentTick, oreCompositions }: Props) {
  const svgRef = useRef<SVGSVGElement>(null)
  const groupRef = useRef<SVGGElement>(null)

  // snapshot, currentTick, oreCompositions accepted for future entity markers
  void snapshot
  void currentTick
  void oreCompositions

  useSvgZoomPan(svgRef, groupRef)

  return (
    <div className="relative w-full h-full bg-void overflow-hidden">
      <svg
        ref={svgRef}
        className="w-full h-full"
        viewBox="-500 -500 1000 1000"
        preserveAspectRatio="xMidYMid meet"
      >
        <g ref={groupRef}>
          {/* Sun at center */}
          <circle cx={0} cy={0} r={12} fill="#f5c842" opacity={0.9} />
          <circle cx={0} cy={0} r={18} fill="none" stroke="#f5c842" opacity={0.2} strokeWidth={4} />

          {/* Orbital rings */}
          {RINGS.map((ring) => (
            <g key={ring.nodeId}>
              <circle
                cx={0}
                cy={0}
                r={ring.radius}
                fill="none"
                stroke="var(--color-edge)"
                strokeWidth={ring.isBelt ? 0.5 : 0.8}
                strokeDasharray={ring.isBelt ? '4 4' : undefined}
                opacity={0.6}
              />
              <text
                x={0}
                y={-ring.radius - 8}
                textAnchor="middle"
                fill="var(--color-label)"
                fontSize={10}
                fontFamily="monospace"
              >
                {ring.label}
              </text>
            </g>
          ))}
        </g>
      </svg>
    </div>
  )
}
