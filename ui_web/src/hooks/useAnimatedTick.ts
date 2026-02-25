import { useEffect, useRef, useState } from 'react'

interface TickSample {
  serverTick: number
  wallTime: number
}

const MAX_SAMPLES = 10
const MAX_LOOKAHEAD_MS = 1000

export function useAnimatedTick(serverTick: number, initialTickRate: number, paused = false) {
  const [displayTick, setDisplayTick] = useState(serverTick)
  const [measuredTickRate, setMeasuredTickRate] = useState(initialTickRate)

  const samplesRef = useRef<TickSample[]>([])
  const rateRef = useRef(initialTickRate)
  const anchorRef = useRef<{ tick: number; wallTime: number }>({ tick: serverTick, wallTime: 0 })

  // Record server tick samples and compute measured rate
  useEffect(() => {
    // Lazy-init wallTime on first effect run (avoids impure call during render)
    if (anchorRef.current.wallTime === 0) {
      anchorRef.current = { tick: serverTick, wallTime: performance.now() }
    }
    const now = performance.now()
    const samples = samplesRef.current

    if (samples.length === 0 || samples[samples.length - 1].serverTick !== serverTick) {
      samples.push({ serverTick, wallTime: now })
      if (samples.length > MAX_SAMPLES) samples.shift()

      anchorRef.current = { tick: serverTick, wallTime: now }

      if (samples.length >= 2) {
        const oldest = samples[0]
        const newest = samples[samples.length - 1]
        const elapsedMs = newest.wallTime - oldest.wallTime
        const elapsedTicks = newest.serverTick - oldest.serverTick
        if (elapsedMs > 0 && elapsedTicks > 0) {
          const rate = (elapsedTicks / elapsedMs) * 1000
          rateRef.current = rate
          setMeasuredTickRate(rate)
        }
      }
    }
  }, [serverTick])

  const pausedRef = useRef(paused)
  useEffect(() => {
    pausedRef.current = paused
  }, [paused])

  // rAF loop for smooth interpolation
  useEffect(() => {
    let rafHandle: number

    function animate() {
      if (pausedRef.current) {
        setDisplayTick(anchorRef.current.tick)
        rafHandle = requestAnimationFrame(animate)
        return
      }
      // Don't interpolate until we have at least 2 server samples
      // (proves the server is actually sending data)
      if (samplesRef.current.length < 2) {
        setDisplayTick(anchorRef.current.tick)
        rafHandle = requestAnimationFrame(animate)
        return
      }
      const now = performance.now()
      const anchor = anchorRef.current
      const elapsedMs = Math.min(now - anchor.wallTime, MAX_LOOKAHEAD_MS)
      const interpolatedTick = anchor.tick + (rateRef.current * elapsedMs) / 1000
      setDisplayTick(interpolatedTick)
      rafHandle = requestAnimationFrame(animate)
    }

    rafHandle = requestAnimationFrame(animate)
    return () => cancelAnimationFrame(rafHandle)
  }, [])

  return { displayTick, measuredTickRate }
}
