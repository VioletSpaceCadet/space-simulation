import { LAB_STATUS_STYLES } from '../config/theme';
import type { LabRateInfo } from '../types';

export interface LabStatusSectionProps {
  labs: LabRateInfo[];
  techNames: Record<string, string>;
}

type LabStatus = 'active' | 'starved' | 'idle';

function getLabStatus(lab: LabRateInfo): LabStatus {
  if (lab.assigned_tech === null) { return 'idle'; }
  if (lab.starved) { return 'starved'; }
  return 'active';
}

interface StatusBadgeProps {
  status: LabStatus;
}

function StatusBadge({ status }: StatusBadgeProps) {
  const { bg, text, label } = LAB_STATUS_STYLES[status] ?? LAB_STATUS_STYLES.idle;

  return (
    <span
      style={{
        background: bg,
        color: text,
        padding: '1px 5px',
        borderRadius: 3,
        fontSize: 10,
        fontWeight: 500,
      }}
    >
      {label}
    </span>
  );
}

export function LabStatusSection({ labs, techNames }: LabStatusSectionProps) {
  if (labs.length === 0) {
    return (
      <div className="text-faint italic text-[11px] px-2 py-1.5">
        no labs
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-0.5 px-2 py-1.5">
      {labs.map((lab) => {
        const status = getLabStatus(lab);
        const techName = lab.assigned_tech ? (techNames[lab.assigned_tech] ?? lab.assigned_tech) : null;

        return (
          <div key={`${lab.station_id}-${lab.module_id}`} className="flex items-center gap-2 text-[11px]">
            <span className="text-label flex-1 min-w-0 truncate">{lab.module_name}</span>
            {techName !== null && (
              <span className="text-muted truncate max-w-[8rem]">{techName}</span>
            )}
            {lab.assigned_tech !== null && (
              <span className="text-[10px]" style={{ color: '#4caf7d' }}>
                +{lab.points_per_hour.toFixed(1)}/hr
              </span>
            )}
            <StatusBadge status={status} />
          </div>
        );
      })}
    </div>
  );
}
