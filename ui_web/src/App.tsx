import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core';
import type { DragEndEvent, DragStartEvent } from '@dnd-kit/core';
import { useCallback, useEffect, useState } from 'react';

import { fetchMeta, pauseGame, resumeGame, setSpeed } from './api';
import { AsteroidTable } from './components/AsteroidTable';
import { EconomyPanel } from './components/EconomyPanel';
import { ErrorBoundary } from './components/ErrorBoundary';
import { EventsFeed } from './components/EventsFeed';
import { FleetPanel } from './components/FleetPanel';
import { FloatingWindow } from './components/FloatingWindow';
import { LayoutRenderer } from './components/LayoutRenderer';
import { RecipeDagPanel } from './components/RecipeDagPanel';
import { ResearchPanel } from './components/ResearchPanel';
import { SolarSystemMapCanvas } from './components/SolarSystemMapCanvas';
import { SPEED_TPS_VALUES, StatusBar } from './components/StatusBar';
import { CopilotMissionBridge } from './copilot/CopilotMissionBridge';
import { useAnimatedTick } from './hooks/useAnimatedTick';
import { useFloatingWindows } from './hooks/useFloatingWindows';
import { useLayoutState } from './hooks/useLayoutState';
import { useSimStream } from './hooks/useSimStream';
import { ALL_PANELS, PANEL_LABELS } from './layout';
import type { PanelId } from './layout';
import { playPause, playResume } from './sounds';

const PANEL_ID_SET: ReadonlySet<string> = new Set<string>(ALL_PANELS);

export default function App() {
  const { snapshot, events, connected, currentTick, activeAlerts, dismissedAlerts, dismissAlert } = useSimStream();
  const { layout, visiblePanels, move, togglePanel, ensurePanelVisible } = useLayoutState();
  const {
    windows: floatingWindows, openWindow, closeWindow,
    updateWindow, bringToFront, closeWindowByPanel,
  } = useFloatingWindows();

  const [ticksPerSec, setTicksPerSec] = useState(10); // default fallback
  const [minutesPerTick, setMinutesPerTick] = useState(60);
  const [paused, setPaused] = useState(false);
  const { displayTick, measuredTickRate } = useAnimatedTick(currentTick, ticksPerSec, paused);

  const [activeDragId, setActiveDragId] = useState<PanelId | null>(null);

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
  );

  useEffect(() => {
    fetchMeta()
      .then((meta) => {
        setTicksPerSec(meta.ticks_per_sec);
        setMinutesPerTick(meta.minutes_per_tick);
        setPaused(meta.paused);
      })
      .catch((err: unknown) => console.error('fetchMeta failed:', err));
  }, []);

  const handleTogglePause = useCallback(() => {
    const nextPaused = !paused;
    if (nextPaused) { playPause(); } else { playResume(); }
    setPaused(nextPaused)
    ;(nextPaused ? pauseGame() : resumeGame()).catch((err: unknown) => {
      console.error('togglePause failed:', err);
      setPaused(!nextPaused);
    });
  }, [paused]);

  const handleSetSpeed = useCallback((tps: number) => {
    setTicksPerSec(tps);
    setSpeed(tps).catch((err: unknown) => {
      console.error('setSpeed failed:', err);
      // Revert on failure — re-fetch meta to get actual speed
      fetchMeta().then((meta) => setTicksPerSec(meta.ticks_per_sec)).catch((innerErr: unknown) => console.error('fetchMeta fallback failed:', innerErr));
    });
  }, []);

  const handleNavigateToPanel = useCallback((panelId: string) => {
    if (PANEL_ID_SET.has(panelId)) {
      ensurePanelVisible(panelId as PanelId);
    }
  }, [ensurePanelVisible]);

  const handlePopOut = useCallback((panelId: PanelId) => {
    // Guard: don't pop out if already floating
    if (floatingWindows.some(w => w.panelId === panelId)) { return; }
    togglePanel(panelId); // Remove from layout
    openWindow(panelId);
  }, [togglePanel, openWindow, floatingWindows]);

  const handleDock = useCallback((windowId: string, panelId: PanelId) => {
    closeWindow(windowId);
    ensurePanelVisible(panelId);
  }, [closeWindow, ensurePanelVisible]);

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const tag = (event.target as HTMLElement)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'BUTTON' || tag === 'SELECT') {return;}
      if (event.code === 'Space') {
        event.preventDefault();
        handleTogglePause();
        return;
      }
      if (event.code === 'ArrowRight' || event.code === 'ArrowLeft') {
        event.preventDefault();
        const currentIndex = SPEED_TPS_VALUES.indexOf(ticksPerSec as typeof SPEED_TPS_VALUES[number]);
        const base = currentIndex === -1 ? 0 : currentIndex;
        if (event.code === 'ArrowLeft') {
          const nextIndex = Math.max(0, base - 1);
          handleSetSpeed(SPEED_TPS_VALUES[nextIndex]);
        } else {
          const nextIndex = Math.min(SPEED_TPS_VALUES.length - 1, base + 1);
          handleSetSpeed(SPEED_TPS_VALUES[nextIndex]);
        }
        return;
      }
    }
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [handleTogglePause, handleSetSpeed, ticksPerSec]);

  const renderPanel = useCallback(
    (id: PanelId) => {
      const content = (() => {
        switch (id) {
          case 'map':
            return <SolarSystemMapCanvas snapshot={snapshot} currentTick={displayTick} />;
          case 'events':
            return <EventsFeed events={events} />;
          case 'asteroids':
            return <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />;
          case 'fleet':
            return (
              <FleetPanel
                ships={snapshot?.ships ?? {}}
                stations={snapshot?.stations ?? {}}
                displayTick={displayTick}
              />
            );
          case 'research':
            return snapshot ? <ResearchPanel research={snapshot.research} /> : null;
          case 'economy':
            return <EconomyPanel snapshot={snapshot} events={events} />;
          case 'manufacturing':
            return <RecipeDagPanel snapshot={snapshot} events={events} currentTick={currentTick} />;
        }
      })();
      return (
        <ErrorBoundary key={id} panelName={PANEL_LABELS[id]}>
          {content}
        </ErrorBoundary>
      );
    },
    [snapshot, events, displayTick, currentTick],
  );

  function handleDragStart(event: DragStartEvent) {
    const panelId = event.active.data.current?.panelId as PanelId | undefined;
    if (panelId) {setActiveDragId(panelId);}
  }

  function handleDragEnd(event: DragEndEvent) {
    const sourcePanelId = event.active.data.current?.panelId as PanelId | undefined;
    const targetPanelId = event.over?.data.current?.targetPanelId as PanelId | undefined;
    const position = event.over?.data.current?.position as string | undefined;

    if (
      sourcePanelId &&
      targetPanelId &&
      position &&
      sourcePanelId !== targetPanelId
    ) {
      move(sourcePanelId, targetPanelId, position as 'before' | 'after' | 'above' | 'below');
    }

    setActiveDragId(null);
  }

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar
        tick={displayTick}
        connected={connected}
        measuredTickRate={measuredTickRate}
        paused={paused}
        balance={snapshot?.balance}
        onTogglePause={handleTogglePause}
        alerts={activeAlerts}
        dismissedAlerts={dismissedAlerts}
        onDismissAlert={dismissAlert}
        onNavigateToPanel={handleNavigateToPanel}
        minutesPerTick={minutesPerTick}
        activeSpeed={ticksPerSec}
        onSetSpeed={handleSetSpeed}
      />
      <div className="flex flex-1 overflow-hidden">
        <nav className="flex flex-col shrink-0 bg-surface border-r border-edge py-2 px-1 gap-0.5">
          {ALL_PANELS.map((id) => {
            const isDocked = visiblePanels.includes(id);
            const isFloating = floatingWindows.some(w => w.panelId === id);
            return (
              <button
                key={id}
                type="button"
                onClick={() => {
                  if (isFloating) {
                    closeWindowByPanel(id);
                  } else {
                    togglePanel(id);
                  }
                }}
                className={`text-[10px] uppercase tracking-widest px-2 py-1.5 rounded-sm transition-colors cursor-pointer text-left ${
                  isDocked || isFloating
                    ? 'text-active bg-edge/40'
                    : 'text-muted hover:text-dim hover:bg-edge/15'
                }`}
              >
                {PANEL_LABELS[id]}{isFloating ? ' ↗' : ''}
              </button>
            );
          })}
        </nav>
        {visiblePanels.length > 0 && (
          <DndContext sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
            <LayoutRenderer
              layout={layout}
              renderPanel={renderPanel}
              isDragging={activeDragId !== null}
              activeDragId={activeDragId}
              onPopOut={handlePopOut}
            />
            <DragOverlay>
              {activeDragId ? (
                <div className="bg-surface border border-accent/50 rounded px-3 py-1 shadow-lg">
                  <span className="text-[11px] uppercase tracking-widest text-accent">
                    {PANEL_LABELS[activeDragId]}
                  </span>
                </div>
              ) : null}
            </DragOverlay>
          </DndContext>
        )}
        {/* Floating windows overlay */}
        {floatingWindows.map((win) => (
          <FloatingWindow
            key={win.id}
            id={win.id}
            panelId={win.panelId}
            x={win.x}
            y={win.y}
            width={win.width}
            height={win.height}
            zIndex={win.zIndex}
            onClose={closeWindow}
            onUpdate={updateWindow}
            onFocus={bringToFront}
            onDock={handleDock}
          >
            {renderPanel(win.panelId)}
          </FloatingWindow>
        ))}
      </div>
      <CopilotMissionBridge
        snapshot={snapshot}
        activeAlerts={activeAlerts}
        currentTick={currentTick}
        paused={paused}
        connected={connected}
      />
    </div>
  );
}
