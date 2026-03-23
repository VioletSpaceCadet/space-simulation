import { ITEM_TYPE_COLORS, RECIPE_STATUS_COLORS } from '../../config/theme';

export function DagLegend() {
  return (
    <div className="absolute bottom-2 left-2 bg-zinc-900/90 rounded px-3 py-2 text-[10px] flex gap-3">
      <span className="flex items-center gap-1">
        <span className="w-2.5 h-2.5 rounded-full inline-block" style={{ background: ITEM_TYPE_COLORS.raw }} />
        Raw
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2.5 h-2.5 rounded-full inline-block" style={{ background: ITEM_TYPE_COLORS.refined }} />
        Refined
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2.5 h-2.5 rounded-full inline-block" style={{ background: ITEM_TYPE_COLORS.component }} />
        Component
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2.5 h-2.5 rounded-full inline-block" style={{ background: ITEM_TYPE_COLORS.ship }} />
        Ship
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2 h-2 rounded-full inline-block" style={{ background: RECIPE_STATUS_COLORS.active }} />
        Active
      </span>
      <span className="flex items-center gap-1">
        <span className="w-2 h-2 rounded-full inline-block" style={{ background: RECIPE_STATUS_COLORS.available }} />
        Available
      </span>
    </div>
  );
}
