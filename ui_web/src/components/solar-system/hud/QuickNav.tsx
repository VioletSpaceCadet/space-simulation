import { useCallback, useEffect, useState } from 'react';

import { BODY_COLORS, IDLE_COLOR, MAP_COLORS } from '../../../config/theme';
import type { OrbitalBodyDef, ShipState, StationState } from '../../../types';
import { getTaskKind } from '../../../utils';

export interface FlyTarget {
  id: string;
  label: string;
  color: string;
  x: number;
  y: number;
  zoom: number;
}

interface QuickNavProps {
  bodies: OrbitalBodyDef[];
  stations: StationState[];
  ships: ShipState[];
  onFlyTo: (target: FlyTarget) => void;
  auUmToWorld: (v: number) => number;
  bodyAbsolutes: Record<string, { x_au_um: number; y_au_um: number }>;
}

/** Zoom level for fly-to based on body type. */
function bodyFlyZoom(bodyType: string): number {
  switch (bodyType) {
    case 'Star': return 0.12;
    case 'Planet': return 6;
    case 'Moon': return 20;
    default: return 0.35;
  }
}

// Reuse BODY_COLORS from theme — same mapping

export function QuickNav({ bodies, stations, ships, onFlyTo, auUmToWorld, bodyAbsolutes }: QuickNavProps) {
  const [modifier, setModifier] = useState<'shift' | 'ctrl' | null>(null);

  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Shift' && !e.ctrlKey && !e.metaKey) {
        setModifier('shift');
      } else if ((e.key === 'Control' || e.key === 'Meta') && !e.shiftKey) {
        setModifier('ctrl');
      }

      const num = parseInt(e.key);
      if (num >= 1 && num <= 9) {
        // Let the parent handle the actual navigation — dispatch custom event
        window.dispatchEvent(new CustomEvent('map-nav-key', { detail: { num, modifier: e.shiftKey ? 'shift' : e.ctrlKey || e.metaKey ? 'ctrl' : null } }));
      }
    }
    function onKeyUp(e: KeyboardEvent) {
      if (e.key === 'Shift' || e.key === 'Control' || e.key === 'Meta') {
        setModifier(null);
      }
    }
    window.addEventListener('keydown', onKeyDown);
    window.addEventListener('keyup', onKeyUp);
    return () => {
      window.removeEventListener('keydown', onKeyDown);
      window.removeEventListener('keyup', onKeyUp);
    };
  }, []);

  // Build waypoints based on modifier
  const buildTargets = useCallback((): FlyTarget[] => {
    if (modifier === 'shift') {
      return stations.map((st) => {
        const abs = bodyAbsolutes[st.position.parent_body] ?? { x_au_um: 0, y_au_um: 0 };
        return {
          id: st.id,
          label: st.id,
          color: MAP_COLORS.stationAccent,
          x: auUmToWorld(abs.x_au_um),
          y: auUmToWorld(abs.y_au_um),
          zoom: 8,
        };
      });
    }
    if (modifier === 'ctrl') {
      return ships.map((sh) => {
        const abs = bodyAbsolutes[sh.position.parent_body] ?? { x_au_um: 0, y_au_um: 0 };
        const kind = getTaskKind(sh.task) ?? 'idle';
        return {
          id: sh.id,
          label: sh.id,
          color: kind === 'idle' ? IDLE_COLOR : MAP_COLORS.stationAccent,
          x: auUmToWorld(abs.x_au_um),
          y: auUmToWorld(abs.y_au_um),
          zoom: 5,
        };
      });
    }
    // Default: location waypoints from bodies
    return bodies
      .filter((b) => b.body_type !== 'Zone' && b.body_type !== 'Belt')
      .map((b) => {
        const abs = bodyAbsolutes[b.id];
        if (!abs) { return null; }
        return {
          id: b.id,
          label: b.name,
          color: BODY_COLORS[b.body_type] ?? '#888',
          x: auUmToWorld(abs.x_au_um),
          y: auUmToWorld(abs.y_au_um),
          zoom: bodyFlyZoom(b.body_type),
        };
      })
      .filter((t): t is FlyTarget => t !== null);
  }, [modifier, bodies, stations, ships, bodyAbsolutes, auUmToWorld]);

  const targets = buildTargets();
  const title = modifier === 'shift' ? 'Stations' : modifier === 'ctrl' ? 'Ships' : 'Navigate';

  // Listen for keyboard nav events
  useEffect(() => {
    function onNavKey(e: Event) {
      const { num, modifier: mod } = (e as CustomEvent).detail;
      const currentTargets = modifier === mod ? targets : [];
      const target = currentTargets[num - 1];
      if (target) { onFlyTo(target); }
    }
    window.addEventListener('map-nav-key', onNavKey);
    return () => window.removeEventListener('map-nav-key', onNavKey);
  }, [targets, modifier, onFlyTo]);

  return (
    <div className="absolute top-4 right-4 z-10 pointer-events-auto">
      <div className="bg-void/88 border border-edge rounded px-2.5 py-1.5 backdrop-blur-sm text-[10px]">
        <div className="uppercase tracking-[1.5px] text-muted text-[9px] mb-1">{title}</div>
        <div className="flex flex-col gap-0.5">
          {targets.slice(0, 9).map((target, index) => (
            <button
              key={target.id}
              type="button"
              className="flex items-center gap-1.5 px-2.5 py-1 rounded-sm text-dim text-left
                         bg-white/[0.03] border border-edge hover:bg-accent/[0.08]
                         hover:border-accent/25 hover:text-accent transition-all cursor-pointer"
              onClick={() => onFlyTo(target)}
            >
              <span className="inline-flex items-center justify-center w-4 h-4 rounded-sm
                              bg-white/[0.04] border border-edge text-[9px] text-muted shrink-0">
                {index + 1}
              </span>
              <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: target.color }} />
              <span className="truncate">{target.label}</span>
            </button>
          ))}
        </div>
        <div className="text-[9px] text-faint mt-1 leading-snug">
          Hold <span className="text-muted">Shift</span> stations
          {' '}&middot;{' '}
          <span className="text-muted">Ctrl</span> ships
        </div>
      </div>
    </div>
  );
}
