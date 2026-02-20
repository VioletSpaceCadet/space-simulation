import { Panel, PanelGroup, PanelResizeHandle } from 'react-resizable-panels'
import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { FleetPanel } from './components/FleetPanel'
import { ResearchPanel } from './components/ResearchPanel'
import { StatusBar } from './components/StatusBar'
import { useSimStream } from './hooks/useSimStream'

export default function App() {
  const { snapshot, events, connected, currentTick, oreCompositions } = useSimStream()

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar tick={currentTick} connected={connected} />
      <PanelGroup direction="horizontal" className="flex-1 overflow-hidden">
        <Panel defaultSize={20} minSize={12}>
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Events</h2>
            <EventsFeed events={events} />
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel defaultSize={40} minSize={20}>
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Asteroids</h2>
            <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel defaultSize={20} minSize={12}>
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Fleet</h2>
            <FleetPanel ships={snapshot?.ships ?? {}} stations={snapshot?.stations ?? {}} oreCompositions={oreCompositions} />
          </section>
        </Panel>
        <PanelResizeHandle className="w-px bg-edge hover:bg-dim cursor-col-resize transition-colors" />
        <Panel defaultSize={20} minSize={12}>
          <section className="flex flex-col h-full overflow-hidden bg-void p-3">
            <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Research</h2>
            {snapshot && <ResearchPanel research={snapshot.research} />}
          </section>
        </Panel>
      </PanelGroup>
    </div>
  )
}
