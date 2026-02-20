import { act, renderHook } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { useSortableData } from './useSortableData'

interface Row {
  id: string
  name: string
  mass: number
}

const data: Row[] = [
  { id: 'c', name: 'Charlie', mass: 300 },
  { id: 'a', name: 'Alpha', mass: 100 },
  { id: 'b', name: 'Bravo', mass: 200 },
]

describe('useSortableData', () => {
  it('returns data in original order when no sort applied', () => {
    const { result } = renderHook(() => useSortableData(data))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['c', 'a', 'b'])
    expect(result.current.sortConfig).toBeNull()
  })

  it('sorts ascending on first click', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['a', 'b', 'c'])
    expect(result.current.sortConfig).toEqual({ key: 'id', direction: 'asc' })
  })

  it('sorts descending on second click', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id'))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['c', 'b', 'a'])
    expect(result.current.sortConfig).toEqual({ key: 'id', direction: 'desc' })
  })

  it('clears sort on third click', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id'))
    expect(result.current.sortedData.map((r) => r.id)).toEqual(['c', 'a', 'b'])
    expect(result.current.sortConfig).toBeNull()
  })

  it('sorts numbers correctly', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('mass'))
    expect(result.current.sortedData.map((r) => r.mass)).toEqual([100, 200, 300])
  })

  it('resets to ascending when switching columns', () => {
    const { result } = renderHook(() => useSortableData(data))
    act(() => result.current.requestSort('id'))
    act(() => result.current.requestSort('id')) // desc
    act(() => result.current.requestSort('mass')) // new column -> asc
    expect(result.current.sortConfig).toEqual({ key: 'mass', direction: 'asc' })
  })

  it('updates when data changes', () => {
    const { result, rerender } = renderHook(
      ({ items }) => useSortableData(items),
      { initialProps: { items: data } },
    )
    act(() => result.current.requestSort('mass'))
    const newData = [...data, { id: 'd', name: 'Delta', mass: 50 }]
    rerender({ items: newData })
    expect(result.current.sortedData[0].mass).toBe(50)
  })
})
