import { createPinia, disposePinia, setActivePinia } from 'pinia'
import { effectScope } from 'vue'
import { expect, it, vi } from 'vitest'
import { deviceInfoGQL } from '@/lib/api/query'
import type { Fragment_DeviceInfoFragment, Fragment_DeviceStatusFragment } from '@/lib/api/graphql/operations'
import { useDeviceInfo } from '@/views/device-info/device-info'

const { gqlFetch } = vi.hoisted(() => ({ gqlFetch: vi.fn() }))
vi.mock('@/lib/api/gql-client', () => ({ gqlFetch }))
vi.mock('@/lib/api/subscription-client', () => ({ graphqlSubscriptions: {} }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key === 'yes' ? 'Yes' : key === 'no' ? 'No' : key }) }))

const deviceInfo: Fragment_DeviceInfoFragment = {
  name: 'Test device', platform: 'LINUX', manufacturer: '', model: '',
  osName: 'Linux', osVersion: '6.12', kernelVersion: '6.12', appVersion: '4.0.0',
  appBuildNumber: '706', language: 'en', cpuArch: 'aarch64', cpuModel: 'Test CPU',
  totalMemory: 8 * 1024 ** 3, totalStorage: 128 * 1024 ** 3,
}

type DeviceStatusResponse = Omit<Fragment_DeviceStatusFragment, 'batteryLevel' | 'memoryAvailable'> & {
  batteryLevel: number | null
  memoryAvailable: number | null
}

const deviceStatus: DeviceStatusResponse = {
  uptimeSec: 90_061, batteryLevel: 80, charging: true,
  temperatures: [{ label: 'battery', celsius: 28.5 }], cpuUsage: 12.34,
  memoryAvailable: 2 * 1024 ** 3, storageAvailable: 40 * 1024 ** 3,
}

async function load(status: Partial<DeviceStatusResponse> = {}, info: Partial<Fragment_DeviceInfoFragment> = {}) {
  const pinia = createPinia()
  setActivePinia(pinia)
  const scope = effectScope()
  gqlFetch.mockReset()
  gqlFetch.mockResolvedValue({ data: { deviceInfo: { ...deviceInfo, ...info }, deviceStatus: { ...deviceStatus, ...status } } })

  try {
    const page = scope.run(useDeviceInfo)!
    await gqlFetch.mock.results[0].value
    expect(gqlFetch).toHaveBeenCalledExactlyOnceWith(deviceInfoGQL, undefined, { fresh: false })
    expect(page.loading.value).toBe(false)
    return {
      status: Object.fromEntries(page.statusInfos.value.map(({ label, value }) => [label, value])),
      hardware: Object.fromEntries(page.hardwareInfos.value.map(({ label, value }) => [label, value])),
    }
  } finally {
    scope.stop()
    disposePinia(pinia)
    setActivePinia(undefined)
  }
}

it('formats device readings and keeps uptime longer than one day', async () => {
  const page = await load()
  expect(page.status).toMatchObject({
    battery_level: '80%', charging: 'Yes', temperatures: ['battery: 28.5 ℃'],
    cpu_usage: '12.3%', memory_available: '2.1 GB', storage_available: '42.9 GB', uptime: '25:01:01',
  })
  expect(page.hardware.cpu_model).toBe('Test CPU')
})

it('omits unavailable readings when Core reports no battery level', async () => {
  const page = await load({ batteryLevel: null, charging: false, temperatures: [], memoryAvailable: null })
  expect(page.status).not.toHaveProperty('battery_level')
  expect(page.status).not.toHaveProperty('temperatures')
  expect(page.status).not.toHaveProperty('memory_available')
  expect(page.status.uptime).toBe('25:01:01')
})

it('preserves a real battery level of zero', async () => {
  const page = await load({ batteryLevel: 0, charging: false })
  expect(page.status).toMatchObject({ battery_level: '0%', charging: 'No' })
})

it('omits an empty CPU model but keeps the CPU architecture', async () => {
  const page = await load({}, { cpuModel: '' })
  expect(page.hardware).not.toHaveProperty('cpu_model')
  expect(page.hardware.cpu_arch).toBe('aarch64')
})
