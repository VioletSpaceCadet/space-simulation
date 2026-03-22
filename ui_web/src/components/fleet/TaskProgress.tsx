import type { ShipState } from '../../types';

export function TaskProgress({ task, displayTick }: { task: ShipState['task']; displayTick: number }) {
  if (!task) {return null;}
  const total = task.eta_tick - task.started_tick;
  if (total <= 0) {return null;}
  const elapsed = Math.max(0, Math.min(displayTick - task.started_tick, total));
  const pctDone = Math.round((elapsed / total) * 100);

  return (
    <div className="flex items-center gap-1.5 min-w-[80px]">
      <div
        role="progressbar"
        aria-valuenow={pctDone}
        aria-valuemin={0}
        aria-valuemax={100}
        className="flex-1 h-1.5 bg-edge rounded-full overflow-hidden"
      >
        <div
          className="h-full bg-accent rounded-full"
          style={{ width: `${pctDone}%` }}
        />
      </div>
      <span className="text-muted text-[10px] w-7 text-right">{pctDone}%</span>
    </div>
  );
}
