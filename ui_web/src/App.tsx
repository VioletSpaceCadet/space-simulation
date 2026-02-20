import { useCallback, useState } from 'react'
import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { FleetPanel } from './components/FleetPanel'
import { PanelHeader } from './components/PanelHeader'
import { ResearchPanel } from './components/ResearchPanel'
import { StatusBar } from './components/StatusBar'
import { useSimStream } from './hooks/useSimStream'

function readCollapsed(key: string): boolean {
  try {
    return localStorage.getItem(`panel:${key}:collapsed`) === 'true'
  } catch {
    return false
  }
}

function writeCollapsed(key: string, collapsed: boolean) {
  try {
    localStorage.setItem(`panel:${key}:collapsed`, String(collapsed))
  } catch {
    // localStorage unavailable
  }
}

function usePanelCollapse(key: string) {
  const [collapsed, setCollapsed] = useState(() => readCollapsed(key))

  const toggle = useCallback(() => {
    setCollapsed((prev) => {
      const next = !prev
      writeCollapsed(key, next)
      return next
    })
  }, [key])

  const onCollapse = useCallback(() => {
    setCollapsed(true)
    writeCollapsed(key, true)
  }, [key])

  const onExpand = useCallback(() => {
    setCollapsed(false)
    writeCollapsed(key, false)
  }, [key])

  return { collapsed, toggle, onCollapse, onExpand }
}

export default function App() {
  const { snapshot, events, connected, currentTick, oreCompositions } = useSimStream()

  const eventsPanel = usePanelCollapse('events')
  const asteroidsPanel = usePanelCollapse('asteroids')
  const fleetPanel = usePanelCollapse('fleet')
  const researchPanel = usePanelCollapse('research')

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar tick={currentTick} connected={connected} />
      <PanelGroup direction="horizontal" className="flex-1 overflow-hidden">
        <Panel
          defaultSize={20}
          minSize={12}
          collapsible
          collapsedSize={0}
          onCollapse={eventsPanel.onCollapse}
          onExpand={eventsPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Events" collapsed={eventsPanel.collapsed} onToggle={eventsPanel.toggle} />
            {!eventsPanel.collapsed && <EventsFeed events={events} />}
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel
          defaultSize={40}
          minSize={20}
          collapsible
          collapsedSize={0}
          onCollapse={asteroidsPanel.onCollapse}
          onExpand={asteroidsPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Asteroids" collapsed={asteroidsPanel.collapsed} onToggle={asteroidsPanel.toggle} />
            {!asteroidsPanel.collapsed && <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />}
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel
          defaultSize={20}
          minSize={12}
          collapsible
          collapsedSize={0}
          onCollapse={fleetPanel.onCollapse}
          onExpand={fleetPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Fleet" collapsed={fleetPanel.collapsed} onToggle={fleetPanel.toggle} />
            {!fleetPanel.collapsed && <FleetPanel ships={snapshot?.ships ?? {}} stations={snapshot?.stations ?? {}} oreCompositions={oreCompositions} />}
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel
          defaultSize={20}
          minSize={12}
          collapsible
          collapsedSize={0}
          onCollapse={researchPanel.onCollapse}
          onExpand={researchPanel.onExpand}
        >
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <PanelHeader title="Research" collapsed={researchPanel.collapsed} onToggle={researchPanel.toggle} />
            {!researchPanel.collapsed && snapshot && <ResearchPanel research={snapshot.research} />}
          </section>
        </Panel>
      </PanelGroup>
    </div>
  )
}
