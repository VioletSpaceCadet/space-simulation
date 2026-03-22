import type { ShipState } from '../../types';
import { getTaskKind } from '../../utils';

import { InventoryDisplay } from './InventoryDisplay';
import { TaskProgress } from './TaskProgress';

export function ShipDetail({ ship, displayTick }: { ship: ShipState; displayTick: number }) {
  const task = ship.task;
  const taskType = getTaskKind(task);

  return (
    <div className="grid grid-cols-[auto_auto] gap-x-8 gap-y-2 text-[11px] w-fit">
      {/* Left: Inventory */}
      <div>
        <div className="text-label text-[10px] uppercase tracking-wider mb-1">Cargo</div>
        {ship.inventory.length === 0 ? (
          <span className="text-faint">empty</span>
        ) : (
          <InventoryDisplay inventory={ship.inventory} />
        )}
      </div>
      {/* Right: Task detail */}
      <div>
        <div className="text-label text-[10px] uppercase tracking-wider mb-1">Task</div>
        {!task ? (
          <span className="text-faint">idle</span>
        ) : (
          <div className="space-y-1">
            <div className="text-fg">{taskType}</div>
            <TaskProgress task={task} displayTick={displayTick} />
            {taskType === 'Transit' && 'Transit' in task.kind && (
              <div className="text-dim">destination: {task.kind.Transit.destination.parent_body}</div>
            )}
            {taskType === 'Mine' && 'Mine' in task.kind && (
              <div className="text-dim">asteroid: {task.kind.Mine.asteroid}</div>
            )}
            {taskType === 'Deposit' && 'Deposit' in task.kind && (
              <>
                <div className="text-dim">station: {task.kind.Deposit.station}</div>
                {task.kind.Deposit.blocked && (
                  <span className="text-[9px] px-1 rounded text-red-400 bg-red-400/10">BLOCKED</span>
                )}
              </>
            )}
            {taskType === 'Survey' && 'Survey' in task.kind && (
              <div className="text-dim">site: {task.kind.Survey.site}</div>
            )}
            {taskType === 'DeepScan' && 'DeepScan' in task.kind && (
              <div className="text-dim">asteroid: {task.kind.DeepScan.asteroid}</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
