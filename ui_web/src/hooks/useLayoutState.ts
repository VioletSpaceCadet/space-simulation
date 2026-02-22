import { useState, useCallback, useMemo } from 'react'
import {
  ALL_PANELS,
  buildDefaultLayout,
  deserializeLayout,
  findPanelIds,
  insertPanel,
  movePanel,
  removePanel,
  serializeLayout,
} from '../layout'
import type { GroupNode, PanelId } from '../layout'

const STORAGE_KEY = 'panel-layout'

type Position = 'before' | 'after' | 'above' | 'below'

function loadLayout(): GroupNode {
  const stored = localStorage.getItem(STORAGE_KEY)
  if (stored) {
    const parsed = deserializeLayout(stored)
    if (parsed && parsed.type === 'group') return parsed
  }
  return buildDefaultLayout(ALL_PANELS)
}

function persist(layout: GroupNode): void {
  localStorage.setItem(STORAGE_KEY, serializeLayout(layout))
}

export function useLayoutState() {
  const [layout, setLayout] = useState<GroupNode>(loadLayout)

  const visiblePanels: PanelId[] = useMemo(() => findPanelIds(layout), [layout])

  const move = useCallback((panelId: PanelId, targetId: PanelId, position: Position) => {
    setLayout((current) => {
      const next = movePanel(current, panelId, targetId, position)
      const result = next.type === 'group' ? next : { ...buildDefaultLayout(ALL_PANELS), children: [next] } as GroupNode
      persist(result)
      return result
    })
  }, [])

  const togglePanel = useCallback((panelId: PanelId) => {
    setLayout((current) => {
      const currentPanels = findPanelIds(current)
      if (currentPanels.includes(panelId)) {
        // Remove — but not if it's the last one
        if (currentPanels.length <= 1) return current
        const result = removePanel(current, panelId)
        // removePanel might collapse to a leaf if only one remains
        const next: GroupNode = result.type === 'group'
          ? result
          : { type: 'group', direction: 'horizontal', children: [result] }
        persist(next)
        return next
      } else {
        // Add after the last visible panel
        const lastPanel = currentPanels[currentPanels.length - 1]
        const result = insertPanel(current, panelId, lastPanel, 'after')
        const next: GroupNode = result.type === 'group'
          ? result
          : { type: 'group', direction: 'horizontal', children: [result] }
        persist(next)
        return next
      }
    })
  }, [])

  const resetLayout = useCallback(() => {
    const defaultLayout = buildDefaultLayout(ALL_PANELS)
    persist(defaultLayout)
    setLayout(defaultLayout)
  }, [])

  return { layout, visiblePanels, move, togglePanel, resetLayout }
}
