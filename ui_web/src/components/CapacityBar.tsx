function formatKg(kg: number): string {
  if (kg >= 1_000_000) {return `${(kg / 1_000_000).toFixed(1)}M`;}
  if (kg >= 1_000) {return `${(kg / 1_000).toFixed(1)}k`;}
  return kg.toLocaleString(undefined, { maximumFractionDigits: 1 });
}

function capacityColor(pct: number): string {
  if (pct >= 90) {return 'bg-red-400';}
  if (pct >= 70) {return 'bg-yellow-400';}
  return 'bg-green-400';
}

interface Props {
  usedKg: number
  capacityKg: number
}

export function CapacityBar({ usedKg, capacityKg }: Props) {
  const pct = capacityKg > 0 ? Math.round((usedKg / capacityKg) * 100) : 0;

  return (
    <div className="flex items-center gap-2 min-w-[120px]">
      <div
        role="progressbar"
        aria-valuenow={pct}
        aria-valuemin={0}
        aria-valuemax={100}
        className="flex-1 h-1.5 bg-edge rounded-full overflow-hidden"
      >
        <div
          data-testid="capacity-fill"
          className={`h-full rounded-full ${capacityColor(pct)}`}
          style={{ width: `${Math.min(pct, 100)}%` }}
        />
      </div>
      <span className="text-dim text-[10px] whitespace-nowrap">
        {pct}% — {formatKg(usedKg)} kg
      </span>
    </div>
  );
}
