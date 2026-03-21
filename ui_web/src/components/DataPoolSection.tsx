const DATA_KIND_LABELS: Record<string, string> = {
  SurveyData: 'Survey',
  AssayData: 'Assay',
  ManufacturingData: 'Manufacturing',
  TransitData: 'Transit',
};

const DATA_KIND_COLORS: Record<string, string> = {
  SurveyData: '#5ca0c8',
  AssayData: '#c89a4a',
  ManufacturingData: '#4caf7d',
  TransitData: '#a78bfa',
};

export interface DataPoolSectionProps {
  dataPool: Record<string, number>;
  dataRates: Record<string, number>;
}

export function DataPoolSection({ dataPool, dataRates }: DataPoolSectionProps) {
  const entries = Object.entries(dataPool);

  if (entries.length === 0) {
    return (
      <div className="text-faint italic text-[11px] px-2 py-1.5">
        no data
      </div>
    );
  }

  return (
    <div className="grid grid-cols-2 gap-x-3 gap-y-1 px-2 py-1.5 text-[11px]">
      {entries.map(([kind, amount]) => {
        const label = DATA_KIND_LABELS[kind] ?? kind;
        const color = DATA_KIND_COLORS[kind] ?? '#888888';
        const rate = dataRates[kind] ?? 0;

        return (
          <div key={kind} className="flex items-center gap-1.5">
            <span style={{ color }} className="font-medium">{label}</span>
            <span className="text-muted">{amount.toFixed(1)}</span>
            {rate !== 0 && (
              <span style={{ color: rate > 0 ? '#4caf7d' : '#e05252' }}>
                {rate > 0 ? '+' : ''}{rate.toFixed(1)}/hr
              </span>
            )}
          </div>
        );
      })}
    </div>
  );
}
