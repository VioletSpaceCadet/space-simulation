import { describe, expect, it } from 'vitest'
import {
  ALL_PANELS,
  PANEL_LABELS,
  buildDefaultLayout,
  findPanelIds,
  removePanel,
  insertPanel,
  movePanel,
  wouldMoveChange,
  serializeLayout,
  deserializeLayout,
  type GroupNode,
  type LeafNode,
} from './layout'

describe('constants', () => {
  it('ALL_PANELS contains all 6 panel ids', () => {
    expect(ALL_PANELS).toEqual(['map', 'events', 'asteroids', 'fleet', 'research', 'economy'])
  })

  it('PANEL_LABELS maps each panel id to a label', () => {
    expect(PANEL_LABELS).toEqual({
      map: 'Map',
      events: 'Events',
      asteroids: 'Asteroids',
      fleet: 'Fleet',
      research: 'Research',
      economy: 'Economy',
    })
  })
})

describe('buildDefaultLayout', () => {
  it('creates a horizontal group with one leaf per panel', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    expect(layout.type).toBe('group')
    expect(layout.direction).toBe('horizontal')
    expect(layout.children).toHaveLength(3)
    expect(layout.children[0]).toEqual({ type: 'leaf', panelId: 'map' })
    expect(layout.children[1]).toEqual({ type: 'leaf', panelId: 'events' })
    expect(layout.children[2]).toEqual({ type: 'leaf', panelId: 'asteroids' })
  })

  it('creates layout for all panels by default', () => {
    const layout = buildDefaultLayout(ALL_PANELS)
    expect(layout.children).toHaveLength(6)
  })

  it('creates layout with single panel', () => {
    const layout = buildDefaultLayout(['map'])
    expect(layout.children).toHaveLength(1)
  })
})

describe('findPanelIds', () => {
  it('collects panel id from a leaf', () => {
    const leaf: LeafNode = { type: 'leaf', panelId: 'map' }
    expect(findPanelIds(leaf)).toEqual(['map'])
  })

  it('collects panel ids from a flat group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'fleet'])
    expect(findPanelIds(layout)).toEqual(['map', 'events', 'fleet'])
  })

  it('collects panel ids from nested layout', () => {
    const nested: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'asteroids' },
          ],
        },
        { type: 'leaf', panelId: 'fleet' },
      ],
    }
    expect(findPanelIds(nested)).toEqual(['map', 'events', 'asteroids', 'fleet'])
  })
})

describe('removePanel', () => {
  it('removes a leaf from a flat group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    const result = removePanel(layout, 'events')
    expect(result.type).toBe('group')
    const group = result as GroupNode
    expect(group.children).toHaveLength(2)
    expect(findPanelIds(group)).toEqual(['map', 'asteroids'])
  })

  it('collapses single-child group after removal', () => {
    const layout = buildDefaultLayout(['map', 'events'])
    const result = removePanel(layout, 'events')
    // Should collapse to just the leaf
    expect(result).toEqual({ type: 'leaf', panelId: 'map' })
  })

  it('collapses nested single-child group', () => {
    const nested: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'asteroids' },
          ],
        },
      ],
    }
    const result = removePanel(nested, 'events')
    // The vertical group should collapse to just asteroids leaf
    const group = result as GroupNode
    expect(group.type).toBe('group')
    expect(group.direction).toBe('horizontal')
    expect(group.children).toHaveLength(2)
    expect(group.children[0]).toEqual({ type: 'leaf', panelId: 'map' })
    expect(group.children[1]).toEqual({ type: 'leaf', panelId: 'asteroids' })
  })

  it('returns node unchanged if panelId not found', () => {
    const layout = buildDefaultLayout(['map', 'events'])
    const result = removePanel(layout, 'fleet')
    expect(result).toEqual(layout)
  })
})

describe('insertPanel', () => {
  it('inserts before a target in the same horizontal group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    const result = insertPanel(layout, 'fleet', 'events', 'before')
    const group = result as GroupNode
    expect(findPanelIds(group)).toEqual(['map', 'fleet', 'events', 'asteroids'])
  })

  it('inserts after a target in the same horizontal group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    const result = insertPanel(layout, 'fleet', 'events', 'after')
    const group = result as GroupNode
    expect(findPanelIds(group)).toEqual(['map', 'events', 'fleet', 'asteroids'])
  })

  it('inserts above a target by wrapping in a vertical group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    const result = insertPanel(layout, 'fleet', 'events', 'above')
    const group = result as GroupNode
    expect(group.type).toBe('group')
    expect(group.direction).toBe('horizontal')
    expect(group.children).toHaveLength(3)
    // The middle child should be a vertical group with fleet above events
    const vertGroup = group.children[1] as GroupNode
    expect(vertGroup.type).toBe('group')
    expect(vertGroup.direction).toBe('vertical')
    expect(vertGroup.children).toHaveLength(2)
    expect(vertGroup.children[0]).toEqual({ type: 'leaf', panelId: 'fleet' })
    expect(vertGroup.children[1]).toEqual({ type: 'leaf', panelId: 'events' })
  })

  it('inserts below a target by wrapping in a vertical group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    const result = insertPanel(layout, 'fleet', 'events', 'below')
    const group = result as GroupNode
    const vertGroup = group.children[1] as GroupNode
    expect(vertGroup.type).toBe('group')
    expect(vertGroup.direction).toBe('vertical')
    expect(vertGroup.children).toHaveLength(2)
    expect(vertGroup.children[0]).toEqual({ type: 'leaf', panelId: 'events' })
    expect(vertGroup.children[1]).toEqual({ type: 'leaf', panelId: 'fleet' })
  })

  it('inserts into existing vertical group when using above/below', () => {
    // Layout: horizontal [ map, vertical [ events, asteroids ] ]
    const layout: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'asteroids' },
          ],
        },
      ],
    }
    const result = insertPanel(layout, 'fleet', 'events', 'above')
    const group = result as GroupNode
    // Should insert into existing vertical group, not nest deeper
    const vertGroup = group.children[1] as GroupNode
    expect(vertGroup.type).toBe('group')
    expect(vertGroup.direction).toBe('vertical')
    expect(vertGroup.children).toHaveLength(3)
    expect(findPanelIds(vertGroup)).toEqual(['fleet', 'events', 'asteroids'])
  })

  it('inserts into existing horizontal group when using before/after', () => {
    // Layout: vertical [ horizontal [ map, events ], asteroids ]
    const layout: GroupNode = {
      type: 'group',
      direction: 'vertical',
      children: [
        {
          type: 'group',
          direction: 'horizontal',
          children: [
            { type: 'leaf', panelId: 'map' },
            { type: 'leaf', panelId: 'events' },
          ],
        },
        { type: 'leaf', panelId: 'asteroids' },
      ],
    }
    const result = insertPanel(layout, 'fleet', 'events', 'after')
    const group = result as GroupNode
    const horizGroup = group.children[0] as GroupNode
    expect(horizGroup.type).toBe('group')
    expect(horizGroup.direction).toBe('horizontal')
    expect(horizGroup.children).toHaveLength(3)
    expect(findPanelIds(horizGroup)).toEqual(['map', 'events', 'fleet'])
  })
})

describe('movePanel', () => {
  it('reorders within same level', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids', 'fleet'])
    const result = movePanel(layout, 'fleet', 'map', 'after')
    expect(findPanelIds(result)).toEqual(['map', 'fleet', 'events', 'asteroids'])
  })

  it('moves panel to a different position with before', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids', 'fleet'])
    const result = movePanel(layout, 'fleet', 'events', 'before')
    expect(findPanelIds(result)).toEqual(['map', 'fleet', 'events', 'asteroids'])
  })

  it('moves panel with above creates vertical group', () => {
    const layout = buildDefaultLayout(['map', 'events', 'asteroids'])
    const result = movePanel(layout, 'asteroids', 'map', 'below')
    // map should be wrapped in a vertical group with asteroids below
    const group = result as GroupNode
    expect(group.direction).toBe('horizontal')
    expect(group.children).toHaveLength(2)
    const vertGroup = group.children[0] as GroupNode
    expect(vertGroup.type).toBe('group')
    expect(vertGroup.direction).toBe('vertical')
    expect(vertGroup.children[0]).toEqual({ type: 'leaf', panelId: 'map' })
    expect(vertGroup.children[1]).toEqual({ type: 'leaf', panelId: 'asteroids' })
  })
})

describe('serializeLayout / deserializeLayout', () => {
  it('round-trips a flat layout', () => {
    const layout = buildDefaultLayout(ALL_PANELS)
    const json = serializeLayout(layout)
    const restored = deserializeLayout(json)
    expect(restored).toEqual(layout)
  })

  it('round-trips a nested layout', () => {
    const nested: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'asteroids' },
          ],
        },
      ],
    }
    const json = serializeLayout(nested)
    const restored = deserializeLayout(json)
    expect(restored).toEqual(nested)
  })

  it('returns null for invalid JSON', () => {
    expect(deserializeLayout('not json')).toBeNull()
  })

  it('returns null for invalid structure — missing type', () => {
    expect(deserializeLayout(JSON.stringify({ panelId: 'map' }))).toBeNull()
  })

  it('returns null for invalid structure — bad type', () => {
    expect(deserializeLayout(JSON.stringify({ type: 'unknown' }))).toBeNull()
  })

  it('returns null for leaf with invalid panelId', () => {
    expect(deserializeLayout(JSON.stringify({ type: 'leaf', panelId: 'bogus' }))).toBeNull()
  })

  it('returns null for group with invalid direction', () => {
    expect(
      deserializeLayout(
        JSON.stringify({ type: 'group', direction: 'diagonal', children: [] }),
      ),
    ).toBeNull()
  })

  it('returns null for group with invalid child', () => {
    expect(
      deserializeLayout(
        JSON.stringify({
          type: 'group',
          direction: 'horizontal',
          children: [{ type: 'leaf', panelId: 'bogus' }],
        }),
      ),
    ).toBeNull()
  })
})

describe('wouldMoveChange', () => {
  // [map, events, fleet] — horizontal
  const horizontal: GroupNode = buildDefaultLayout(['map', 'events', 'fleet'])

  it('returns false for adjacent "after" (already next to each other)', () => {
    // fleet is already after events
    expect(wouldMoveChange(horizontal, 'fleet', 'events', 'after')).toBe(false)
  })

  it('returns false for adjacent "before" (already next to each other)', () => {
    // map is already before events
    expect(wouldMoveChange(horizontal, 'map', 'events', 'before')).toBe(false)
  })

  it('returns true for non-adjacent "before"', () => {
    // fleet is far right, moving before map changes layout
    expect(wouldMoveChange(horizontal, 'fleet', 'map', 'before')).toBe(true)
  })

  it('returns true for cross-axis moves (above/below) in horizontal layout', () => {
    expect(wouldMoveChange(horizontal, 'fleet', 'events', 'above')).toBe(true)
    expect(wouldMoveChange(horizontal, 'fleet', 'events', 'below')).toBe(true)
  })

  it('returns true for non-adjacent "after"', () => {
    // map is far left, moving after fleet changes layout
    expect(wouldMoveChange(horizontal, 'map', 'fleet', 'after')).toBe(true)
  })

  it('handles nested layouts', () => {
    // [map, [events, fleet vertical]] — events is above fleet
    const nested: GroupNode = {
      type: 'group',
      direction: 'horizontal',
      children: [
        { type: 'leaf', panelId: 'map' },
        {
          type: 'group',
          direction: 'vertical',
          children: [
            { type: 'leaf', panelId: 'events' },
            { type: 'leaf', panelId: 'fleet' },
          ],
        },
      ],
    }
    // fleet is already below events in the vertical group
    expect(wouldMoveChange(nested, 'fleet', 'events', 'below')).toBe(false)
    // but moving fleet above events would change it
    expect(wouldMoveChange(nested, 'fleet', 'events', 'above')).toBe(true)
    // moving fleet before map is a real change
    expect(wouldMoveChange(nested, 'fleet', 'map', 'before')).toBe(true)
  })

  it('returns true for two-panel layout same-axis swap', () => {
    const twoPanel: GroupNode = buildDefaultLayout(['map', 'events'])
    expect(wouldMoveChange(twoPanel, 'map', 'events', 'after')).toBe(true)
    expect(wouldMoveChange(twoPanel, 'events', 'map', 'before')).toBe(true)
  })
})
