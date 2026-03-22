import { useCallback, useEffect, useRef } from 'react';

import { BODY_COLORS, ZONE_COLORS } from '../../../config/theme';
import type { AbsolutePos, OrbitalBodyDef } from '../../../types';
import type { Camera } from '../canvas/types';
import { auUmToWorld } from '../canvas/types';

const MINIMAP_SIZE = 140;
/** World units visible in the minimap (roughly 12 AU diameter). */
const MINIMAP_EXTENT = 12 * 200; // 12 AU * 200 world units per AU

interface MinimapProps {
  bodies: OrbitalBodyDef[];
  bodyAbsolutes: Record<string, AbsolutePos>;
  camera: Camera;
  viewWidth: number;
  viewHeight: number;
  onNavigate: (worldX: number, worldY: number) => void;
}

export function Minimap({ bodies, bodyAbsolutes, camera, viewWidth, viewHeight, onNavigate }: MinimapProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const draggingRef = useRef(false);

  // Convert world coords to minimap pixel coords
  const toMini = useCallback((worldX: number, worldY: number) => {
    const scale = MINIMAP_SIZE / MINIMAP_EXTENT;
    return {
      mx: worldX * scale + MINIMAP_SIZE / 2,
      my: worldY * scale + MINIMAP_SIZE / 2,
    };
  }, []);

  // Convert minimap pixel coords to world coords
  const fromMini = useCallback((mx: number, my: number) => {
    const scale = MINIMAP_SIZE / MINIMAP_EXTENT;
    return {
      wx: (mx - MINIMAP_SIZE / 2) / scale,
      wy: (my - MINIMAP_SIZE / 2) / scale,
    };
  }, []);

  // Draw minimap
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) { return; }
    const dpr = window.devicePixelRatio || 1;
    canvas.width = MINIMAP_SIZE * dpr;
    canvas.height = MINIMAP_SIZE * dpr;
    const ctx = canvas.getContext('2d');
    if (!ctx) { return; }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, MINIMAP_SIZE, MINIMAP_SIZE);

    // Draw zones
    for (const body of bodies) {
      if (!body.zone) { continue; }
      const parentAbs = bodyAbsolutes[body.parent ?? ''] ?? bodyAbsolutes[body.id];
      if (!parentAbs) { continue; }
      const center = toMini(auUmToWorld(parentAbs.x_au_um), auUmToWorld(parentAbs.y_au_um));
      const scale = MINIMAP_SIZE / MINIMAP_EXTENT;
      const rMax = auUmToWorld(body.zone.radius_max_au_um) * scale;
      const rMin = auUmToWorld(body.zone.radius_min_au_um) * scale;
      if (rMax < 1) { continue; }
      if (body.zone.angle_span_mdeg < 360_000) { continue; } // only full-circle zones on minimap
      ctx.beginPath();
      ctx.arc(center.mx, center.my, rMax, 0, Math.PI * 2);
      ctx.arc(center.mx, center.my, rMin, 0, Math.PI * 2, true);
      ctx.fillStyle = ZONE_COLORS[body.zone.resource_class] ?? ZONE_COLORS.Mixed;
      ctx.fill();
    }

    // Draw bodies
    for (const body of bodies) {
      if (body.body_type === 'Zone' || body.body_type === 'Belt') { continue; }
      const abs = bodyAbsolutes[body.id];
      if (!abs) { continue; }
      const { mx, my } = toMini(auUmToWorld(abs.x_au_um), auUmToWorld(abs.y_au_um));
      const r = body.body_type === 'Star' ? 3 : 1.5;
      ctx.beginPath();
      ctx.arc(mx, my, r, 0, Math.PI * 2);
      ctx.fillStyle = BODY_COLORS[body.body_type] ?? '#888';
      ctx.fill();
    }

    // Draw viewport rectangle
    const halfW = (viewWidth / camera.zoom) / 2;
    const halfH = (viewHeight / camera.zoom) / 2;
    const tl = toMini(camera.x - halfW, camera.y - halfH);
    const br = toMini(camera.x + halfW, camera.y + halfH);
    const rw = br.mx - tl.mx;
    const rh = br.my - tl.my;
    ctx.strokeStyle = '#5ca0c8';
    ctx.globalAlpha = 0.5;
    ctx.lineWidth = 1;
    ctx.strokeRect(
      Math.max(0, Math.min(MINIMAP_SIZE - 6, tl.mx)),
      Math.max(0, Math.min(MINIMAP_SIZE - 6, tl.my)),
      Math.max(6, Math.min(MINIMAP_SIZE, rw)),
      Math.max(6, Math.min(MINIMAP_SIZE, rh)),
    );
    ctx.globalAlpha = 1;
  }, [bodies, bodyAbsolutes, camera, viewWidth, viewHeight, toMini]);

  const handleMinimapClick = useCallback((e: React.MouseEvent) => {
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const { wx, wy } = fromMini(mx, my);
    onNavigate(wx, wy);
  }, [fromMini, onNavigate]);

  const handleMinimapDrag = useCallback((e: React.MouseEvent) => {
    if (!draggingRef.current) { return; }
    const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const { wx, wy } = fromMini(mx, my);
    onNavigate(wx, wy);
  }, [fromMini, onNavigate]);

  return (
    <div
      className="absolute bottom-4 right-4 z-10 pointer-events-auto"
    >
      {/* eslint-disable-next-line jsx-a11y/no-static-element-interactions, jsx-a11y/click-events-have-key-events */}
      <div
        className="bg-void/90 border border-edge rounded overflow-hidden backdrop-blur-sm cursor-crosshair"
        style={{ width: MINIMAP_SIZE, height: MINIMAP_SIZE }}
        onClick={handleMinimapClick}
        onMouseDown={() => { draggingRef.current = true; }}
        onMouseMove={handleMinimapDrag}
        onMouseUp={() => { draggingRef.current = false; }}
        onMouseLeave={() => { draggingRef.current = false; }}
      >
        <canvas
          ref={canvasRef}
          style={{ width: MINIMAP_SIZE, height: MINIMAP_SIZE }}
        />
      </div>
    </div>
  );
}
