export function SortIndicator({ column, sortConfig }: {
  column: string
  sortConfig: { key: string; direction: string } | null
}) {
  if (!sortConfig || sortConfig.key !== column) {
    return <span className="text-faint/40 ml-1">⇅</span>;
  }
  return (
    <span className="text-accent ml-1">
      {sortConfig.direction === 'asc' ? '▲' : '▼'}
    </span>
  );
}
