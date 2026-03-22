import { useCallback, useEffect, useRef, useState } from 'react';

import { fetchSpatialConfig } from '../api';
import type {
  AbsolutePos,
  SimSnapshot,
  SolarSystemConfig,
} from '../types';
import { entityAbsolute } from '../utils/spatial';

import { screenToWorld, worldToScreen, lerpCamera } from './solar-system/canvas/camera';
import { drawMap, shipAbsolutePos } from './solar-system/canvas/renderer';
import { generateStarTile } from './solar-system/canvas/starfield';
import type { Camera } from './solar-system/canvas/types';
import {
  INITIAL_CAMERA,
  MAX_ZOOM,
  MIN_ZOOM,
  PARALLAX_FACTOR,
  STAR_TILE_SIZE,
  ZOOM_IN_RATIO,
  ZOOM_OUT_RATIO,
  auUmToWorld,
  smoothStep,
} from './solar-system/canvas/types';
import type { EntityInfo } from './solar-system/DetailCard';
import { DetailCard } from './solar-system/DetailCard';
import { Tooltip } from './solar-system/Tooltip';

interface Props {
  snapshot: SimSnapshot | null;
  currentTick: number;
}

/** Hit test radius in screen pixels. */
const HIT_RADIUS = 12;

/** Minimum drag distance (px) before suppressing click. */
const DRAG_THRESHOLD = 4;

export function SolarSystemMapCanvas({ snapshot, currentTick }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const starBgRef = useRef<HTMLDivElement>(null);
  const animFrameRef = useRef(0);

  const cameraRef = useRef<Camera>({ ...INITIAL_CAMERA });
  const targetCameraRef = useRef<Camera>({ ...INITIAL_CAMERA });
  const sizeRef = useRef({ width: 0, height: 0 });

  // Dragging state — use state for cursor since it affects render
  const draggingRef = useRef(false);
  const lastMouseRef = useRef({ x: 0, y: 0 });
  const dragStartRef = useRef({ x: 0, y: 0 });
  const didDragRef = useRef(false);
  const [dragging, setDragging] = useState(false);

  const [config, setConfig] = useState<SolarSystemConfig | null>(null);
  const [hovered, setHovered] = useState<{
    type: string;
    id: string;
    screenX: number;
    screenY: number;
  } | null>(null);
  const [selected, setSelected] = useState<{
    type: string;
    id: string;
  } | null>(null);

  // Keep snapshot/tick in refs for animation loop access
  const snapshotRef = useRef(snapshot);
  const currentTickRef = useRef(currentTick);
  const configRef = useRef(config);
  useEffect(() => {
    snapshotRef.current = snapshot;
  }, [snapshot]);
  useEffect(() => {
    currentTickRef.current = currentTick;
  }, [currentTick]);
  useEffect(() => {
    configRef.current = config;
  }, [config]);

  // Merged body absolutes
  const bodyAbsolutes = {
    ...(config?.body_absolutes ?? {}),
    ...(snapshot?.body_absolutes ?? {}),
  };
  const bodyAbsolutesRef = useRef(bodyAbsolutes);
  useEffect(() => {
    bodyAbsolutesRef.current = bodyAbsolutes;
  });

  // --- Fetch spatial config ---
  useEffect(() => {
    let cancelled = false;
    fetchSpatialConfig()
      .then((c) => {
        if (!cancelled) { setConfig(c); }
      })
      .catch((err: unknown) =>
        console.error('Failed to load spatial config:', err),
      );
    return () => {
      cancelled = true;
    };
  }, []);

  // --- Generate star tile (lazy, runs once) ---
  const [starTileUrl] = useState(generateStarTile);

  // --- Canvas sizing ---
  useEffect(() => {
    const container = containerRef.current;
    if (!container) { return; }

    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        sizeRef.current = { width, height };
        const canvas = canvasRef.current;
        if (canvas) {
          const dpr = window.devicePixelRatio || 1;
          canvas.width = width * dpr;
          canvas.height = height * dpr;
          canvas.style.width = `${width}px`;
          canvas.style.height = `${height}px`;
        }
      }
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  // --- Wheel handler (native, non-passive to allow preventDefault) ---
  useEffect(() => {
    const container = containerRef.current;
    if (!container) { return; }

    function onWheel(e: WheelEvent) {
      e.preventDefault();
      const target = targetCameraRef.current;
      const { width, height } = sizeRef.current;

      const rect = container!.getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;

      const before = screenToWorld(mouseX, mouseY, target, width, height);
      const ratio = e.deltaY < 0 ? ZOOM_IN_RATIO : ZOOM_OUT_RATIO;
      target.zoom = Math.max(MIN_ZOOM, Math.min(MAX_ZOOM, target.zoom * ratio));
      const after = screenToWorld(mouseX, mouseY, target, width, height);
      target.x += before.wx - after.wx;
      target.y += before.wy - after.wy;
    }

    container.addEventListener('wheel', onWheel, { passive: false });
    return () => container.removeEventListener('wheel', onWheel);
  }, []);

  // --- Animation loop ---
  useEffect(() => {
    function frame() {
      animFrameRef.current = requestAnimationFrame(frame);

      const canvas = canvasRef.current;
      const cfg = configRef.current;
      if (!canvas || !cfg) { return; }

      const ctx = canvas.getContext('2d');
      if (!ctx) { return; }

      const { width, height } = sizeRef.current;
      if (width === 0 || height === 0) { return; }

      lerpCamera(cameraRef.current, targetCameraRef.current);

      const dpr = window.devicePixelRatio || 1;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

      if (starBgRef.current) {
        const cam = cameraRef.current;
        const ox = -(cam.x * PARALLAX_FACTOR) % STAR_TILE_SIZE;
        const oy = -(cam.y * PARALLAX_FACTOR) % STAR_TILE_SIZE;
        starBgRef.current.style.backgroundPosition = `${ox}px ${oy}px`;
      }

      const snap = snapshotRef.current;
      const stations = snap ? Object.values(snap.stations) : [];
      const ships = snap ? Object.values(snap.ships) : [];
      const asteroids = snap ? Object.values(snap.asteroids) : [];
      const scanSites = snap ? snap.scan_sites : [];

      drawMap(
        {
          ctx,
          camera: cameraRef.current,
          width,
          height,
          config: cfg,
          bodyAbsolutes: bodyAbsolutesRef.current,
          currentTick: currentTickRef.current,
        },
        stations,
        ships,
        asteroids,
        scanSites,
      );
    }

    animFrameRef.current = requestAnimationFrame(frame);
    return () => cancelAnimationFrame(animFrameRef.current);
  }, []);

  // --- Entity lookup ---
  const lookupEntity = useCallback(
    (
      sel: { type: string; id: string },
      snap: SimSnapshot,
    ): EntityInfo | null => {
      if (sel.type === 'station' && snap.stations[sel.id]) {
        return { type: 'station', data: snap.stations[sel.id] };
      }
      if (sel.type === 'ship' && snap.ships[sel.id]) {
        return { type: 'ship', data: snap.ships[sel.id] };
      }
      if (sel.type === 'asteroid' && snap.asteroids[sel.id]) {
        return { type: 'asteroid', data: snap.asteroids[sel.id] };
      }
      if (sel.type === 'scan-site') {
        const site = snap.scan_sites.find((s) => s.id === sel.id);
        if (site) { return { type: 'scan-site', data: site }; }
      }
      return null;
    },
    [],
  );

  // --- Hit testing ---
  const hitTest = useCallback(
    (
      screenX: number,
      screenY: number,
    ): { type: string; id: string } | null => {
      const snap = snapshotRef.current;
      const cfg = configRef.current;
      if (!snap || !cfg) { return null; }

      const { width, height } = sizeRef.current;
      const cam = cameraRef.current;
      const ba = bodyAbsolutesRef.current;

      const zoom = cam.zoom;
      let closestDist = HIT_RADIUS;
      let closestHit: { type: string; id: string } | null = null;

      // Helper: check entity at screen position
      function check(type: string, id: string, abs: AbsolutePos) {
        const w = { x: auUmToWorld(abs.x_au_um), y: auUmToWorld(abs.y_au_um) };
        const s = worldToScreen(w.x, w.y, cam, width, height);
        const dist = Math.hypot(s.sx - screenX, s.sy - screenY);
        if (dist < closestDist) {
          closestDist = dist;
          closestHit = { type, id };
        }
      }

      // Stations — visible from system zoom (as dots)
      for (const station of Object.values(snap.stations)) {
        check('station', station.id, entityAbsolute(station.position, ba));
      }

      // Ships — visible at region+ zoom (0.15+)
      if (smoothStep(zoom, 0.1, 0.2) > 0.01) {
        for (const ship of Object.values(snap.ships)) {
          check('ship', ship.id, shipAbsolutePos(ship, {
            bodyAbsolutes: ba,
            currentTick: currentTickRef.current,
          }));
        }
      }

      // Asteroids — match renderer threshold (fadeIn=0.25, fullIn=0.7)
      if (smoothStep(zoom, 0.25, 0.7) > 0.01) {
        for (const asteroid of Object.values(snap.asteroids)) {
          check('asteroid', asteroid.id, entityAbsolute(asteroid.position, ba));
        }
      }

      // Scan sites — match renderer threshold (fadeIn=0.3, fullIn=0.8)
      if (smoothStep(zoom, 0.3, 0.8) > 0.01) {
        for (const site of snap.scan_sites) {
          check('scan-site', site.id, entityAbsolute(site.position, ba));
        }
      }

      return closestHit;
    },
    [],
  );

  // --- Mouse handlers ---
  const handleMouseDown = useCallback((e: React.MouseEvent) => {
    if (e.button !== 0) { return; }
    draggingRef.current = true;
    didDragRef.current = false;
    setDragging(true);
    lastMouseRef.current = { x: e.clientX, y: e.clientY };
    dragStartRef.current = { x: e.clientX, y: e.clientY };
  }, []);

  const handleMouseMove = useCallback(
    (e: React.MouseEvent) => {
      if (draggingRef.current) {
        const dx = e.clientX - lastMouseRef.current.x;
        const dy = e.clientY - lastMouseRef.current.y;
        lastMouseRef.current = { x: e.clientX, y: e.clientY };

        // Track if we've moved enough to count as a drag
        const totalDx = e.clientX - dragStartRef.current.x;
        const totalDy = e.clientY - dragStartRef.current.y;
        if (Math.hypot(totalDx, totalDy) > DRAG_THRESHOLD) {
          didDragRef.current = true;
        }

        const target = targetCameraRef.current;
        target.x -= dx / target.zoom;
        target.y -= dy / target.zoom;
        return;
      }

      // Hover hit test
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;
      const hit = hitTest(mouseX, mouseY);

      if (hit) {
        setHovered({
          type: hit.type,
          id: hit.id,
          screenX: e.clientX,
          screenY: e.clientY,
        });
      } else {
        setHovered(null);
      }
    },
    [hitTest],
  );

  const handleMouseUp = useCallback(() => {
    draggingRef.current = false;
    setDragging(false);
  }, []);

  const handleClick = useCallback(
    (e: React.MouseEvent) => {
      // Suppress click if this was a drag gesture
      if (didDragRef.current) { return; }

      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;
      const hit = hitTest(mouseX, mouseY);

      if (hit) {
        setSelected(hit);
      } else {
        setSelected(null);
      }
    },
    [hitTest],
  );

  // --- Double-click: zoom to entity (bodies + stations only) ---
  const handleDoubleClick = useCallback(
    (e: React.MouseEvent) => {
      const rect = (e.currentTarget as HTMLElement).getBoundingClientRect();
      const mouseX = e.clientX - rect.left;
      const mouseY = e.clientY - rect.top;

      const { width, height } = sizeRef.current;
      const cam = cameraRef.current;
      const ba = bodyAbsolutesRef.current;
      const cfg = configRef.current;

      // Check bodies first
      if (cfg) {
        for (const body of cfg.bodies) {
          if (body.body_type === 'Zone' || body.body_type === 'Belt') { continue; }
          const abs = ba[body.id];
          if (!abs) { continue; }
          const w = { x: auUmToWorld(abs.x_au_um), y: auUmToWorld(abs.y_au_um) };
          const s = worldToScreen(w.x, w.y, cam, width, height);
          if (Math.hypot(s.sx - mouseX, s.sy - mouseY) < HIT_RADIUS * 2) {
            const target = targetCameraRef.current;
            target.x = w.x;
            target.y = w.y;
            target.zoom = Math.min(MAX_ZOOM, cam.zoom * 2);
            return;
          }
        }
      }

      // Check stations
      const snap = snapshotRef.current;
      if (snap) {
        for (const station of Object.values(snap.stations)) {
          const abs = entityAbsolute(station.position, ba);
          const w = { x: auUmToWorld(abs.x_au_um), y: auUmToWorld(abs.y_au_um) };
          const s = worldToScreen(w.x, w.y, cam, width, height);
          if (Math.hypot(s.sx - mouseX, s.sy - mouseY) < HIT_RADIUS * 2) {
            const target = targetCameraRef.current;
            target.x = w.x;
            target.y = w.y;
            target.zoom = Math.min(MAX_ZOOM, cam.zoom * 2);
            return;
          }
        }
      }
    },
    [],
  );

  // --- Tooltip content ---
  const tooltipContent = (() => {
    if (!hovered || !snapshot) { return null; }
    const entity = lookupEntity(hovered, snapshot);
    if (!entity) { return null; }
    return (
      <Tooltip x={hovered.screenX} y={hovered.screenY}>
        <div className="text-accent">{entity.data.id}</div>
        <div className="text-dim">{entity.type}</div>
      </Tooltip>
    );
  })();

  // --- Detail card ---
  const detailCard = (() => {
    if (!selected || !snapshot) { return null; }
    const entity = lookupEntity(selected, snapshot);
    if (!entity) { return null; }
    return <DetailCard entity={entity} onClose={() => setSelected(null)} />;
  })();

  return (
    /* eslint-disable jsx-a11y/no-static-element-interactions, jsx-a11y/click-events-have-key-events */
    <div
      ref={containerRef}
      className="relative w-full h-full overflow-hidden outline-none"
      style={{
        background: 'var(--color-void)',
        cursor: dragging ? 'grabbing' : 'grab',
      }}
      onMouseDown={handleMouseDown}
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      onMouseLeave={handleMouseUp}
      onClick={handleClick}
      onDoubleClick={handleDoubleClick}
    >
      {/* Starfield CSS background with parallax */}
      <div
        ref={starBgRef}
        className="absolute inset-0"
        style={{
          backgroundImage: starTileUrl ? `url(${starTileUrl})` : undefined,
          backgroundSize: `${STAR_TILE_SIZE}px`,
          backgroundRepeat: 'repeat',
          imageRendering: 'pixelated',
        }}
      />

      {/* Main map canvas */}
      <canvas ref={canvasRef} className="absolute inset-0" />

      {/* DOM overlays */}
      {tooltipContent}
      {detailCard}
    </div>
  );
}
