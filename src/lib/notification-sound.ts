/**
 * Plays a soft macOS-style chime notification sound using the Web Audio API.
 * Approximates a bell/glass chime with two inharmonic partials, a fast attack
 * and a natural exponential decay — similar to the macOS "Glass" alert tone.
 */
export function playNotificationSound(volume = 1): void {
  try {
    const ctx = new AudioContext()
    const master = ctx.createGain()
    master.gain.value = Math.max(0, Math.min(1, volume))
    master.connect(ctx.destination)

    // Two partials give the characteristic inharmonic "bell" colour
    const partials: Array<{ freq: number; gain: number; decay: number }> = [
      { freq: 1047, gain: 0.55, decay: 0.9 }, // C6 — fundamental
      { freq: 1480, gain: 0.30, decay: 0.6 }, // ~F#6 — inharmonic upper partial
      { freq: 2093, gain: 0.12, decay: 0.35 }, // C7 — gentle shimmer
    ]

    const attackTime = 0.008 // 8 ms — near-instant, like a struck bell
    const durationSec = 1.1

    partials.forEach(({ freq, gain, decay }) => {
      const osc = ctx.createOscillator()
      const env = ctx.createGain()

      osc.type = 'sine'
      osc.frequency.value = freq

      // ADSR-style envelope: fast attack → exponential decay
      env.gain.setValueAtTime(0.0001, ctx.currentTime)
      env.gain.linearRampToValueAtTime(gain, ctx.currentTime + attackTime)
      env.gain.exponentialRampToValueAtTime(0.0001, ctx.currentTime + decay)

      osc.connect(env)
      env.connect(master)

      osc.start(ctx.currentTime)
      osc.stop(ctx.currentTime + durationSec)
    })

    // Close the context shortly after all oscillators have finished
    setTimeout(() => ctx.close(), (durationSec + 0.1) * 1000)
  } catch {
    // AudioContext unavailable (e.g. secure-context restriction)
  }
}
