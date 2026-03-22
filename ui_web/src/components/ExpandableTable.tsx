import React, { useState } from 'react';

import { useSortableData } from '../hooks/useSortableData';

import { SortIndicator } from './SortIndicator';

const HEADER_CLASS =
  'text-left text-label px-2 py-1 border-b border-edge font-normal cursor-pointer hover:text-dim transition-colors select-none';
const HEADER_CLASS_STATIC =
  'text-left text-label px-2 py-1 border-b border-edge font-normal select-none';
const CELL_CLASS = 'px-2 py-0.5 border-b border-surface';

export interface ColumnDef<T> {
  key: string
  label: string
  sortable?: boolean
  render: (row: T) => React.ReactNode
}

export function ExpandableTable<T extends { id: string }>({
  data,
  columns,
  renderDetail,
}: {
  data: T[]
  columns: ColumnDef<T>[]
  renderDetail: (row: T) => React.ReactNode
}) {
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const sortableRows = data.map((row) => {
    const sortable: Record<string, unknown> = { id: row.id };
    for (const col of columns) {sortable[col.key] = col.key === 'id' ? row.id : (row as Record<string, unknown>)[col.key];}
    return { ...sortable, _row: row };
  });

  const { sortedData, sortConfig, requestSort: requestSortTyped } = useSortableData(sortableRows);
  const requestSort = requestSortTyped as (key: string) => void;
  const colSpan = columns.length;

  return (
    <table className="w-full border-collapse text-[11px]">
      <thead>
        <tr>
          {columns.map((col) => (
            <th
              key={col.key}
              className={col.sortable !== false ? HEADER_CLASS : HEADER_CLASS_STATIC}
              onClick={col.sortable !== false ? () => requestSort(col.key) : undefined}
            >
              {col.label}
              {col.sortable !== false && <SortIndicator column={col.key} sortConfig={sortConfig} />}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {sortedData.map((sortableRow) => {
          const row = sortableRow._row as T;
          const isExpanded = expandedId === row.id;
          return (
            <React.Fragment key={row.id}>
              <tr
                className={`cursor-pointer hover:bg-surface/50 ${isExpanded ? 'bg-surface/60' : ''}`}
                onClick={() => setExpandedId(isExpanded ? null : row.id)}
              >
                {columns.map((col, colIndex) => (
                  <td
                    key={col.key}
                    className={`${CELL_CLASS} ${colIndex === 0 && isExpanded ? 'border-l-2 border-l-accent' : ''}`}
                  >
                    {col.render(row)}
                  </td>
                ))}
              </tr>
              {isExpanded && (
                <tr>
                  <td colSpan={colSpan} className="px-3 py-3 border-b border-surface border-l-2 border-l-accent bg-void/30">
                    {renderDetail(row)}
                  </td>
                </tr>
              )}
            </React.Fragment>
          );
        })}
      </tbody>
    </table>
  );
}
