import { useEffect } from 'react'
import { select } from 'd3-selection'
import { zoom, zoomIdentity } from 'd3-zoom'
import type { D3ZoomEvent } from 'd3-zoom'

interface ZoomPanOptions {
  minZoom?: number
  maxZoom?: number
}

export function useSvgZoomPan(
  svgRef: React.RefObject<SVGSVGElement | null>,
  groupRef: React.RefObject<SVGGElement | null>,
  options: ZoomPanOptions = {},
) {
  const { minZoom = 0.3, maxZoom = 5 } = options

  useEffect(() => {
    const svgEl = svgRef.current
    const groupEl = groupRef.current
    if (!svgEl || !groupEl) return

    const svgSelection = select<SVGSVGElement, unknown>(svgEl)
    const zoomBehavior = zoom<SVGSVGElement, unknown>()
      .scaleExtent([minZoom, maxZoom])
      .on('zoom', (event: D3ZoomEvent<SVGSVGElement, unknown>) => {
        select(groupEl).attr('transform', event.transform.toString())
      })

    svgSelection.call(zoomBehavior)
    svgSelection.call(zoomBehavior.transform, zoomIdentity)

    return () => {
      svgSelection.on('.zoom', null)
    }
  }, [svgRef, groupRef, minZoom, maxZoom])
}
