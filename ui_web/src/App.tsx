import { useCallback, useEffect, useState } from 'react'
import {
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
} from '@dnd-kit/core'
import type { DragEndEvent, DragStartEvent } from '@dnd-kit/core'
import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { FleetPanel } from './components/FleetPanel'
import { LayoutRenderer } from './components/LayoutRenderer'
import { ResearchPanel } from './components/ResearchPanel'
import { SolarSystemMap } from './components/SolarSystemMap'
import { StatusBar } from './components/StatusBar'
import { fetchMeta, pauseGame, resumeGame } from './api'
import { useAnimatedTick } from './hooks/useAnimatedTick'
import { useLayoutState } from './hooks/useLayoutState'
import { useSimStream } from './hooks/useSimStream'
import { ALL_PANELS, PANEL_LABELS } from './layout'
import type { PanelId } from './layout'

export default function App() {
  const { snapshot, events, connected, currentTick, activeAlerts, dismissedAlerts, dismissAlert } = useSimStream()
  const { layout, visiblePanels, move, togglePanel } = useLayoutState()

  const [ticksPerSec, setTicksPerSec] = useState(10) // default fallback
  const [paused, setPaused] = useState(false)
  const { displayTick, measuredTickRate } = useAnimatedTick(currentTick, ticksPerSec, paused)

  const [activeDragId, setActiveDragId] = useState<PanelId | null>(null)

  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 8 } }),
  )

  useEffect(() => {
    fetchMeta()
      .then((meta) => {
        setTicksPerSec(meta.ticks_per_sec)
        setPaused(meta.paused)
      })
      .catch(() => {})
  }, [])

  const handleTogglePause = useCallback(() => {
    const nextPaused = !paused
    setPaused(nextPaused)
    ;(nextPaused ? pauseGame() : resumeGame()).catch(() => setPaused(!nextPaused))
  }, [paused])

  useEffect(() => {
    function handleKeyDown(event: KeyboardEvent) {
      const tag = (event.target as HTMLElement)?.tagName
      if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'BUTTON' || tag === 'SELECT') return
      if (event.code === 'Space') {
        event.preventDefault()
        handleTogglePause()
      }
    }
    window.addEventListener('keydown', handleKeyDown)
    return () => window.removeEventListener('keydown', handleKeyDown)
  }, [handleTogglePause])

  const renderPanel = useCallback(
    (id: PanelId) => {
      switch (id) {
        case 'map':
          return <SolarSystemMap snapshot={snapshot} currentTick={displayTick} oreCompositions={{}} />
        case 'events':
          return <EventsFeed events={events} />
        case 'asteroids':
          return <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />
        case 'fleet':
          return (
            <FleetPanel
              ships={snapshot?.ships ?? {}}
              stations={snapshot?.stations ?? {}}
              displayTick={displayTick}
            />
          )
        case 'research':
          return snapshot ? <ResearchPanel research={snapshot.research} /> : null
      }
    },
    [snapshot, events, displayTick],
  )

  function handleDragStart(event: DragStartEvent) {
    const panelId = event.active.data.current?.panelId as PanelId | undefined
    if (panelId) setActiveDragId(panelId)
  }

  function handleDragEnd(event: DragEndEvent) {
    const sourcePanelId = event.active.data.current?.panelId as PanelId | undefined
    const targetPanelId = event.over?.data.current?.targetPanelId as PanelId | undefined
    const position = event.over?.data.current?.position as string | undefined

    if (
      sourcePanelId &&
      targetPanelId &&
      position &&
      sourcePanelId !== targetPanelId
    ) {
      move(sourcePanelId, targetPanelId, position as 'before' | 'after' | 'above' | 'below')
    }

    setActiveDragId(null)
  }

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar tick={displayTick} connected={connected} measuredTickRate={measuredTickRate} paused={paused} onTogglePause={handleTogglePause} alerts={activeAlerts} dismissedAlerts={dismissedAlerts} onDismissAlert={dismissAlert} />
      <div className="flex flex-1 overflow-hidden">
        <nav className="flex flex-col shrink-0 bg-surface border-r border-edge py-2 px-1 gap-0.5">
          {ALL_PANELS.map((id) => (
            <button
              key={id}
              type="button"
              onClick={() => togglePanel(id)}
              className={`text-[10px] uppercase tracking-widest px-2 py-1.5 rounded-sm transition-colors cursor-pointer text-left ${
                visiblePanels.includes(id)
                  ? 'text-active bg-edge/40'
                  : 'text-muted hover:text-dim hover:bg-edge/15'
              }`}
            >
              {PANEL_LABELS[id]}
            </button>
          ))}
        </nav>
        {visiblePanels.length > 0 && (
        <DndContext sensors={sensors} onDragStart={handleDragStart} onDragEnd={handleDragEnd}>
          <LayoutRenderer layout={layout} renderPanel={renderPanel} isDragging={activeDragId !== null} activeDragId={activeDragId} />
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
      </div>
    </div>
  )
}
