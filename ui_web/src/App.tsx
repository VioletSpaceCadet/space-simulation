import './App.css'
import { AsteroidTable } from './components/AsteroidTable'
import { EventsFeed } from './components/EventsFeed'
import { ResearchPanel } from './components/ResearchPanel'
import { StatusBar } from './components/StatusBar'
import { useSimStream } from './hooks/useSimStream'

export default function App() {
  const { snapshot, events, connected, currentTick } = useSimStream()

  return (
    <div className="app">
      <StatusBar tick={currentTick} connected={connected} />
      <div className="panels">
        <section className="panel panel-events">
          <h2>Events</h2>
          <EventsFeed events={events} />
        </section>
        <section className="panel panel-asteroids">
          <h2>Asteroids</h2>
          <AsteroidTable asteroids={snapshot?.asteroids ?? {}} />
        </section>
        <section className="panel panel-research">
          <h2>Research</h2>
          {snapshot && <ResearchPanel research={snapshot.research} />}
        </section>
      </div>
    </div>
  )
}
