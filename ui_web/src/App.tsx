import { useCallback, useEffect, useState } from 'react'
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { FleetPanel } from './components/FleetPanel'
import { ResearchPanel } from './components/ResearchPanel'
import { SolarSystemMap } from './components/SolarSystemMap'
import { StatusBar } from './components/StatusBar'
import { fetchMeta, pauseGame, resumeGame } from './api'
import { useAnimatedTick } from './hooks/useAnimatedTick'
import { useSimStream } from './hooks/useSimStream'

type PanelId = 'map' | 'events' | 'asteroids' | 'fleet' | 'research'

const PANEL_LABELS: Record<PanelId, string> = {
  map: 'Map',
  events: 'Events',
  asteroids: 'Asteroids',
  fleet: 'Fleet',
  research: 'Research',
}

const ALL_PANELS: PanelId[] = ['map', 'events', 'asteroids', 'fleet', 'research']

function readVisiblePanels(): Set<PanelId> {
  try {
    const stored = localStorage.getItem('visible-panels')
    if (stored) {
      const parsed = JSON.parse(stored) as PanelId[]
      if (Array.isArray(parsed) && parsed.length > 0) return new Set(parsed)
    }
  } catch {
    // ignore
  }
  return new Set(ALL_PANELS)
}

function writeVisiblePanels(visible: Set<PanelId>) {
  try {
    localStorage.setItem('visible-panels', JSON.stringify([...visible]))
  } catch {
    // localStorage unavailable
  }
}

function useVisiblePanels() {
  const [visible, setVisible] = useState<Set<PanelId>>(readVisiblePanels)

  const toggle = useCallback((id: PanelId) => {
    setVisible((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        if (next.size > 1) next.delete(id)
      } else {
        next.add(id)
      }
      writeVisiblePanels(next)
      return next
    })
  }, [])

  return { visible, toggle }
}

export default function App() {
  const { snapshot, events, connected, currentTick, activeAlerts, dismissedAlerts, dismissAlert } = useSimStream()
  const { visible, toggle } = useVisiblePanels()

  const [ticksPerSec, setTicksPerSec] = useState(10) // default fallback
  const [paused, setPaused] = useState(false)
  const { displayTick, measuredTickRate } = useAnimatedTick(currentTick, ticksPerSec)

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

  const visiblePanels = ALL_PANELS.filter((id) => visible.has(id))

  function renderPanel(id: PanelId) {
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
              onClick={() => toggle(id)}
              className={`text-[10px] uppercase tracking-widest px-2 py-1.5 rounded-sm transition-colors cursor-pointer text-left ${
                visible.has(id)
                  ? 'text-active bg-edge/40'
                  : 'text-muted hover:text-dim hover:bg-edge/15'
              }`}
            >
              {PANEL_LABELS[id]}
            </button>
          ))}
        </nav>
        {visiblePanels.length > 0 && (
          <PanelGroup direction="horizontal" className="flex-1 overflow-hidden">
            {visiblePanels.map((id, index) => (
              <div key={id} className="contents">
                {index > 0 && (
                  <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
                )}
                <Panel defaultSize={100 / visiblePanels.length} minSize={10}>
                  <section className="flex flex-col h-full overflow-hidden bg-void p-3">
                    <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">
                      {PANEL_LABELS[id]}
                    </h2>
                    {renderPanel(id)}
                  </section>
                </Panel>
              </div>
            ))}
          </PanelGroup>
        )}
      </div>
    </div>
  )
}
