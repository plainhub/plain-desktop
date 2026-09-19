import { beforeEach, describe, expect, it, vi } from 'vitest'
import { flushPromises, mount } from '@vue/test-utils'
import { defineComponent } from 'vue'
import { createI18n } from 'vue-i18n'

const gqlFetchMock = vi.fn()
vi.mock('@/lib/api/gql-client', () => {
  class GqlError extends Error {}
  return { gqlFetch: (...args: any[]) => gqlFetchMock(...args), GqlError }
})

import { useDeviceInfo } from '@/views/device-info/device-info'
import { deviceInfoGQL, deviceStatusGQL } from '@/lib/api/query'
import { appFragment, deviceInfoFragment, deviceStatusFragment } from '@/lib/api/fragments'
import { formatSeconds, formatFileSize } from '@/lib/format'

const i18n = createI18n({
  legacy: false,
  locale: 'en',
  messages: { en: { yes: 'Yes', no: 'No' } },
})

function mountHook() {
  let hook!: ReturnType<typeof useDeviceInfo>
  mount(defineComponent({
    setup() {
      hook = useDeviceInfo()
      return () => null
    },
  }), { global: { plugins: [i18n] } })
  return hook
}

function deviceStatusPayload(statusOverrides: Record<string, any> = {}, infoOverrides: Record<string, any> = {}) {
  return {
    deviceInfo: {
      name: 'My Phone', platform: 'ANDROID', manufacturer: 'Google', model: 'Pixel 8',
      osName: 'Android', osVersion: '15', kernelVersion: '6.1.75', appVersion: '1.0', appBuildNumber: '100',
      language: 'en', cpuArch: 'arm64-v8a', cpuModel: 'SM8550', totalMemory: 8 * 1024 * 1024 * 1024,
      totalStorage: 128 * 1024 * 1024 * 1024, display: null, android: null,
      ...infoOverrides,
    },
    deviceStatus: {
      uptimeSec: 3661,
      batteryLevel: 80,
      charging: true,
      temperatures: [{ label: 'battery', celsius: 28.5 }],
      cpuUsage: 12.34,
      memoryAvailable: 2 * 1024 * 1024 * 1024,
      storageAvailable: 40 * 1024 * 1024 * 1024,
      ...statusOverrides,
    },
    sims: [],
  }
}

beforeEach(() => {
  gqlFetchMock.mockReset()
  gqlFetchMock.mockResolvedValue({ data: deviceStatusPayload() })
})

describe('device info GraphQL documents', () => {
  it('deviceInfoGQL queries deviceStatus and drops battery/uptime/desktop/android extras', () => {
    expect(deviceInfoGQL).toContain('deviceStatus {')
    expect(deviceInfoGQL).toContain('...DeviceStatusFragment')
    // Removed surface — a regression here makes the whole query error out.
    expect(deviceInfoGQL).not.toContain('battery {')
    expect(deviceInfoGQL).not.toMatch(/^\s+uptime$/m)
    expect(deviceInfoGQL).not.toMatch(/^\s+desktop\s*\{/m)
    expect(deviceInfoGQL).not.toContain('buildHost')
    expect(deviceInfoGQL).not.toContain('buildUser')
    expect(deviceInfoGQL).not.toContain('serial')
    expect(deviceInfoGQL).not.toContain('product')
  })

  it('deviceInfoFragment carries top-level cpuModel and the trimmed field set', () => {
    expect(deviceInfoFragment).toContain('cpuModel')
    expect(deviceInfoFragment).not.toContain('uptime')
    expect(deviceInfoFragment).not.toContain('desktop')
    expect(deviceInfoFragment).not.toContain('buildHost')
    expect(deviceInfoFragment).not.toContain('buildUser')
    expect(deviceInfoFragment).not.toContain('serial')
    expect(deviceInfoFragment).not.toContain('product')
  })

  it('deviceStatusFragment covers all seven DeviceStatus fields', () => {
    for (const f of ['uptimeSec', 'batteryLevel', 'charging', 'temperatures', 'cpuUsage', 'memoryAvailable', 'storageAvailable']) {
      expect(deviceStatusFragment).toContain(f)
    }
    expect(deviceStatusFragment).toContain('celsius')
  })

  it('deviceStatusGQL stays lightweight (battery only, no status collection fields)', () => {
    expect(deviceStatusGQL).toContain('batteryLevel')
    expect(deviceStatusGQL).not.toContain('cpuUsage')
    expect(deviceStatusGQL).not.toContain('temperatures')
    expect(deviceStatusGQL).not.toContain('memoryAvailable')
  })

  it('appFragment no longer selects App.battery', () => {
    expect(appFragment).not.toMatch(/^\s+battery$/m)
  })
})

describe('useDeviceInfo', () => {
  it('builds statusInfos from deviceStatus in contract order', async () => {
    const hook = mountHook()
    await flushPromises()
    expect(hook.statusInfos.value.map((i) => i.label)).toEqual([
      'battery_level', 'charging', 'temperatures', 'cpu_usage', 'memory_available', 'storage_available', 'uptime',
    ])
    const items = Object.fromEntries(hook.statusInfos.value.map((i) => [i.label, i.value]))
    expect(items.battery_level).toBe('80%')
    expect(items.charging).toBe('Yes')
    expect(items.temperatures).toEqual(['battery: 28.5 ℃'])
    expect(items.cpu_usage).toBe('12.3%')
    expect(items.memory_available).toBe(formatFileSize(2 * 1024 * 1024 * 1024))
    expect(items.storage_available).toBe(formatFileSize(40 * 1024 * 1024 * 1024))
    // uptimeSec is seconds — no /1000 anywhere anymore.
    expect(items.uptime).toBe(formatSeconds(3661))
  })

  it('filters null/empty status rows (battery-less devices)', async () => {
    gqlFetchMock.mockResolvedValue({
      data: deviceStatusPayload({
        batteryLevel: null, charging: false, temperatures: [], memoryAvailable: null,
      }),
    })
    const hook = mountHook()
    await flushPromises()
    const labels = hook.statusInfos.value.map((i) => i.label)
    expect(labels).toEqual(['charging', 'cpu_usage', 'storage_available', 'uptime'])
    expect(Object.fromEntries(hook.statusInfos.value.map((i) => [i.label, i.value])).charging).toBe('No')
  })

  it('moves cpuModel into hardwareInfos and keeps uptime out of systemInfos', async () => {
    const hook = mountHook()
    await flushPromises()
    expect(hook.systemInfos.value.map((i) => i.label)).toEqual(['os_name', 'os_version', 'kernel'])
    const hw = Object.fromEntries(hook.hardwareInfos.value.map((i) => [i.label, i.value]))
    expect(hw.cpu_model).toBe('SM8550')
  })

  it('filters an empty cpuModel row', async () => {
    gqlFetchMock.mockResolvedValue({
      data: deviceStatusPayload({}, { cpuModel: null }),
    })
    const hook = mountHook()
    await flushPromises()
    expect(hook.hardwareInfos.value.map((i) => i.label)).not.toContain('cpu_model')
  })

  it('keeps the android platform branch and drops the desktop branch', async () => {
    const data = deviceStatusPayload()
    data.deviceInfo.android = {
      sdkVersion: 35, versionCodeName: 'REL', securityPatch: '2025-01-05', bootloader: 'b1',
      fingerprint: 'fp', hardware: 'qcom', radioVersion: 'r1', board: 'board', buildBrand: 'google',
      buildNumber: 'BP1A', device: 'shiba', javaVmVersion: '21', glEsVersion: '3.2', buildTime: '2025-01-01T00:00:00Z',
    }
    gqlFetchMock.mockResolvedValue({ data })
    const hook = mountHook()
    await flushPromises()
    const labels = hook.platformInfos.value.map((i) => i.label)
    expect(labels).toContain('android_version')
    // The desktop sub-object is gone from the schema; non-android devices
    // simply get no platform card.
    expect(labels).not.toContain('hostname')
    expect(labels).not.toContain('gpu_model')
    expect(labels).not.toContain('window_manager')
  })
})
