import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { ResearchPanel } from './components/ResearchPanel'
import { StatusBar } from './components/StatusBar'
import { useSimStream } from './hooks/useSimStream'

export default function App() {
  const { snapshot, events, connected, currentTick } = useSimStream()

  return (
    <div className="flex flex-col h-screen overflow-hidden">
      <StatusBar tick={currentTick} connected={connected} />
      <div className="flex flex-1 overflow-hidden gap-px bg-edge">
        <section className="flex flex-col overflow-hidden bg-void p-3 flex-1 min-w-[220px]">
          <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Events</h2>
          <EventsFeed events={events} />
        </section>
        <section className="flex flex-col overflow-hidden bg-void p-3 [flex:2]">
          <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Asteroids</h2>
          <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />
        </section>
        <section className="flex flex-col overflow-hidden bg-void p-3 flex-1 min-w-[220px]">
          <h2 className="text-[11px] uppercase tracking-widest text-label mb-2 pb-1.5 border-b border-edge shrink-0">Research</h2>
          {snapshot && <ResearchPanel research={snapshot.research} />}
        </section>
      </div>
    </div>
  )
}
