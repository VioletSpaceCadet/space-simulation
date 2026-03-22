import { useMemo } from 'react';

import { useContent } from '../hooks/useContent';
import type { ResearchState } from '../types';

import { DataPoolSection } from './DataPoolSection';
import { LabStatusSection } from './LabStatusSection';
import { TechTreeDAG } from './TechTreeDAG';

interface Props {
  research: ResearchState;
}

export function ResearchPanel({ research }: Props) {
  const { content } = useContent();

  const techNames = useMemo(() => {
    if (!content) { return {}; }
    return Object.fromEntries(content.techs.map((tech) => [tech.id, tech.name]));
  }, [content]);

  const labAssignments = useMemo(() => {
    if (!content) { return []; }
    return content.lab_rates
      .filter((lab) => lab.assigned_tech !== null)
      .map((lab) => lab.assigned_tech as string);
  }, [content]);

  if (!content) {
    return (
      <div className="overflow-y-auto flex-1 flex items-center justify-center">
        <span className="text-faint italic text-[11px]">loading…</span>
      </div>
    );
  }

  return (
    <div className="overflow-y-auto flex-1 flex flex-col">
      <DataPoolSection dataPool={research.data_pool} dataRates={content.data_rates} />
      <div className="flex-1 min-h-0 overflow-auto">
        <TechTreeDAG techs={content.techs} research={research} labAssignments={labAssignments} />
      </div>
      <LabStatusSection labs={content.lab_rates} techNames={techNames} />
    </div>
  );
}
