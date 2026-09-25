import { afterEach, describe, expect, it, vi } from 'vitest'
import { useMdns } from '@/views/device-info/use-mdns'

const invoke = vi.hoisted(() => vi.fn())

vi.mock('@tauri-apps/api/core', () => ({ invoke }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))

afterEach(() => {
  vi.useRealTimers()
  invoke.mockReset()
})

describe('useMdns', () => {
  it('updates services and activity while open, pauses updates, and stops browsing on close', async () => {
    vi.useFakeTimers()
    let version = 0
    invoke.mockImplementation(async (command: string) => {
      if (command === 'mdns_start_browse') return true
      if (command === 'mdns_snapshot') return [{ instanceFqdn: `phone-${++version}` }]
      if (command === 'mdns_activity') return [{ time: version, event: 'Service found', detail: 'phone' }]
      return true
    })

    const mdns = useMdns()
    await mdns.startBrowsing()
    expect(mdns.snapshots.value[0].instanceFqdn).toBe('phone-1')
    expect(mdns.activity.value[0].time).toBe(1)

    await vi.advanceTimersByTimeAsync(2000)
    expect(mdns.snapshots.value[0].instanceFqdn).toBe('phone-2')
    mdns.paused.value = true
    await vi.advanceTimersByTimeAsync(2000)
    expect(mdns.snapshots.value[0].instanceFqdn).toBe('phone-2')
    mdns.paused.value = false
    await vi.advanceTimersByTimeAsync(2000)
    expect(mdns.activity.value[0].time).toBe(3)

    mdns.stopBrowsing()
    expect(invoke).toHaveBeenCalledWith('mdns_stop_browse')
  })

  it('does not leave browsing running when the dialog closes before startup finishes', async () => {
    vi.useFakeTimers()
    let finishStart!: (value: boolean) => void
    const startResult = new Promise<boolean>((resolve) => { finishStart = resolve })
    invoke.mockImplementation((command: string) => {
      if (command === 'mdns_start_browse') return startResult
      return Promise.resolve(true)
    })

    const mdns = useMdns()
    const starting = mdns.startBrowsing()
    mdns.stopBrowsing()
    finishStart(true)
    await starting
    await vi.advanceTimersByTimeAsync(2000)

    expect(invoke).toHaveBeenCalledWith('mdns_stop_browse')
    expect(invoke).not.toHaveBeenCalledWith('mdns_snapshot')
  })
})
