import { beforeEach, expect, it, vi } from 'vitest'

const mock = vi.hoisted(() => ({
  mounted: [] as Array<() => void>,
  unmounted: [] as Array<() => void>,
  callbacks: new Map<string, (value: any) => void>(),
  refetch: vi.fn(),
  sound: vi.fn(),
  mutate: vi.fn(),
  notification: vi.fn().mockResolvedValue(undefined),
}))
vi.mock('vue', async (original) => ({
  ...await original<typeof import('vue')>(),
  onMounted: (fn: () => void) => mock.mounted.push(fn),
  onUnmounted: (fn: () => void) => mock.unmounted.push(fn),
}))
vi.mock('vue-i18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }))
vi.mock('@/components/toaster', () => ({ default: vi.fn() }))
vi.mock('@/utils/pomodoro', () => ({ playNotificationSound: mock.sound }))
vi.mock('@/lib/desktop-notification', () => ({ showDesktopNotification: mock.notification }))
vi.mock('@/lib/api/query', () => ({ initQuery: () => ({ refetch: mock.refetch }), pomodoroTodayAndSettingsGQL: '' }))
vi.mock('@/lib/api/mutation', () => ({
  initMutation: () => ({ mutate: mock.mutate }),
  startPomodoroGQL: '', stopPomodoroGQL: '', pausePomodoroGQL: '',
}))
vi.mock('@/lib/api/subscription-client', () => ({
  graphqlSubscriptions: {
    subscribe(field: string, callback: (value: any) => void) {
      mock.callbacks.set(field, callback)
      return () => mock.callbacks.delete(field)
    },
  },
}))

import { usePomodoro } from '../../src/views/pomodoro/pomodoro'

beforeEach(() => {
  mock.mounted.length = 0
  mock.unmounted.length = 0
  mock.callbacks.clear()
  vi.clearAllMocks()
  vi.stubGlobal('__IS_TAURI__', true)
})

it('applies action updates, refreshes settings, and unsubscribes', () => {
  const pomodoro = usePomodoro()
  mock.mounted.forEach(callback => callback())
  mock.callbacks.get('pomodoroAction')!({
    today: { completedCount: 1, currentRound: 2, timeLeftSec: 300, totalTimeSec: 300, isRunning: false, isPaused: false, state: 'SHORT_BREAK' },
    completedState: 'WORK',
  })
  expect(pomodoro.currentPhase.value).toBe('shortBreak')
  expect(pomodoro.completedToday.value).toBe(1)
  expect(mock.sound).toHaveBeenCalledOnce()
  expect(mock.notification).toHaveBeenCalledOnce()
  mock.callbacks.get('pomodoroSettingsChanged')!(true)
  expect(mock.refetch).toHaveBeenCalledOnce()
  mock.unmounted.forEach(callback => callback())
  expect(mock.callbacks.size).toBe(0)
})

it('uses each control response immediately without an extra state query', async () => {
  const pomodoro = usePomodoro()
  const today = { completedCount: 2, currentRound: 3, timeLeftSec: 45, totalTimeSec: 60, isRunning: true, isPaused: false, state: 'WORK' }
  mock.mutate.mockResolvedValueOnce({ data: { startPomodoro: today } })
  await pomodoro.startTimer()
  expect(pomodoro.timeLeft.value).toBe(45)
  expect(pomodoro.isRunning.value).toBe(true)
  mock.mutate.mockResolvedValueOnce({ data: { pausePomodoro: { ...today, isRunning: false, isPaused: true } } })
  await pomodoro.pauseTimer()
  expect(pomodoro.isPaused.value).toBe(true)
  mock.mutate.mockResolvedValueOnce({ data: { stopPomodoro: { ...today, timeLeftSec: 60, isRunning: false, isPaused: false } } })
  await pomodoro.stopTimer()
  expect(pomodoro.timeLeft.value).toBe(60)
  expect(pomodoro.isPaused.value).toBe(false)
  expect(pomodoro.completedToday.value).toBe(2)
  expect(mock.refetch).not.toHaveBeenCalled()
})
