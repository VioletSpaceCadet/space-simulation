import { useCallback, useEffect, useState } from 'react'
import { saveGame } from '../api'
import type { ActiveAlert } from '../types'
import { AlertBadges } from './AlertBadges'

interface Props {
  tick: number
  connected: boolean
  measuredTickRate: number
  paused: boolean
  onTogglePause: () => void
  alerts: Map<string, ActiveAlert>
  dismissedAlerts: Set<string>
  onDismissAlert: (alertId: string) => void
}

type SaveStatus = 'idle' | 'saving' | 'saved' | 'error'

export function StatusBar({ tick, connected, measuredTickRate, paused, onTogglePause, alerts, dismissedAlerts, onDismissAlert }: Props) {
  const roundedTick = Math.floor(tick)
  const day = Math.floor(roundedTick / 1440)
  const hour = Math.floor((roundedTick % 1440) / 60)
  const minute = roundedTick % 60

  const [saveStatus, setSaveStatus] = useState<SaveStatus>('idle')

  const handleSave = useCallback(() => {
    setSaveStatus('saving')
    saveGame()
      .then(() => {
        setSaveStatus('saved')
        setTimeout(() => setSaveStatus('idle'), 2000)
      })
      .catch(() => {
        setSaveStatus('error')
        setTimeout(() => setSaveStatus('idle'), 3000)
      })
  }, [])

  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 's' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault()
        handleSave()
      }
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [handleSave])

  const saveLabel =
    saveStatus === 'saving'
      ? 'Saving...'
      : saveStatus === 'saved'
        ? 'Saved'
        : saveStatus === 'error'
          ? 'Save failed'
          : 'Save'

  return (
    <div className="flex gap-6 items-center px-4 py-1.5 bg-surface border-b border-edge text-xs shrink-0">
      <span className="text-accent font-bold">tick {roundedTick}</span>
      <span className="text-dim">
        day {day} | {String(hour).padStart(2, '0')}:{String(minute).padStart(2, '0')}
      </span>
      <span className="text-muted">~{measuredTickRate.toFixed(1)} t/s</span>
      <span className={connected ? 'text-online' : 'text-offline'}>
        {connected ? '● connected' : '○ reconnecting...'}
      </span>
      <div className="ml-auto flex items-center gap-3">
        <AlertBadges alerts={alerts} dismissed={dismissedAlerts} onDismiss={onDismissAlert} />
        <button
          type="button"
          onClick={onTogglePause}
          className={`px-2.5 py-0.5 rounded-sm text-[10px] uppercase tracking-widest transition-colors cursor-pointer border ${
            paused
              ? 'border-accent/40 text-accent'
              : 'border-edge text-muted hover:text-dim hover:border-dim'
          }`}
        >
          {paused ? 'Paused' : 'Running'}
        </button>
        <button
          type="button"
          onClick={handleSave}
          disabled={saveStatus === 'saving'}
          className={`px-2.5 py-0.5 rounded-sm text-[10px] uppercase tracking-widest transition-colors cursor-pointer border ${
            saveStatus === 'saved'
              ? 'border-online/40 text-online'
              : saveStatus === 'error'
                ? 'border-offline/40 text-offline'
                : 'border-edge text-muted hover:text-dim hover:border-dim'
          } disabled:opacity-50 disabled:cursor-not-allowed`}
        >
          {saveLabel}
        </button>
      </div>
    </div>
  )
}
