import { useMemo, useState } from 'react';

export type SortDirection = 'asc' | 'desc'

export interface SortConfig<T> {
  key: keyof T & string
  direction: SortDirection
}

export interface SortableResult<T> {
  sortedData: T[]
  sortConfig: SortConfig<T> | null
  requestSort: (key: keyof T & string) => void
}

export function useSortableData<T>(data: T[]): SortableResult<T> {
  const [sortConfig, setSortConfig] = useState<SortConfig<T> | null>(null);

  const sortedData = useMemo(() => {
    if (!sortConfig) {return data;}

    const { key, direction } = sortConfig;
    return [...data].sort((a, b) => {
      const aVal = a[key];
      const bVal = b[key];
      if (aVal == null && bVal == null) {return 0;}
      if (aVal == null) {return 1;}
      if (bVal == null) {return -1;}

      let cmp: number;
      if (typeof aVal === 'number' && typeof bVal === 'number') {
        cmp = aVal - bVal;
      } else {
        cmp = String(aVal).localeCompare(String(bVal));
      }
      return direction === 'asc' ? cmp : -cmp;
    });
  }, [data, sortConfig]);

  function requestSort(key: keyof T & string) {
    setSortConfig((prev) => {
      if (!prev || prev.key !== key) {return { key, direction: 'asc' };}
      if (prev.direction === 'asc') {return { key, direction: 'desc' };}
      return null;
    });
  }

  return { sortedData, sortConfig, requestSort };
}
