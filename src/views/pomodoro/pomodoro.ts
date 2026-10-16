import { ref, computed, onMounted, onUnmounted } from 'vue'
import { useI18n } from 'vue-i18n'
import toast from '@/components/toaster'
import type { PomodoroSettings } from '@/types/pomodoro'
import { playNotificationSound } from '@/utils/pomodoro'
import { initQuery } from '@/lib/api/query'
import { pomodoroTodayAndSettingsGQL } from '@/lib/api/query'
import { initMutation } from '@/lib/api/mutation'
import { startPomodoroGQL, stopPomodoroGQL, pausePomodoroGQL } from '@/lib/api/mutation'
import emitter from '@/plugins/eventbus'
import { showDesktopNotification } from '@/lib/desktop-notification'

const STATE_MAP: Record<string, 'work' | 'shortBreak' | 'longBreak'> = {
  WORK: 'work',
  SHORT_BREAK: 'shortBreak',
  LONG_BREAK: 'longBreak',
}

export function usePomodoro() {
  const { t } = useI18n()

  const { mutate: startPomodoroMutation } = initMutation({ document: startPomodoroGQL })
  const { mutate: stopPomodoroMutation } = initMutation({ document: stopPomodoroGQL })
  const { mutate: pausePomodoroMutation } = initMutation({ document: pausePomodoroGQL })

  const settings = ref<PomodoroSettings>({
    workDurationMin: 25,
    shortBreakDurationMin: 5,
    longBreakDurationMin: 15,
    pomodorosBeforeLongBreak: 4,
    showNotification: true,
    playSoundOnComplete: true,
  })
  const isRunning = ref(false)
  const isPaused = ref(false)
  const timeLeft = ref(0)
  const totalTime = ref(0)
  const currentPhase = ref<'work' | 'shortBreak' | 'longBreak'>('work')
  const currentRound = ref(1)
  const completedToday = ref(0)

  function applySnapshot(today: any) {
    if (!today) return
    completedToday.value = today.completedCount
    currentRound.value = today.currentRound
    timeLeft.value = today.timeLeftSec
    totalTime.value = today.totalTimeSec
    isRunning.value = today.isRunning
    isPaused.value = today.isPaused
    currentPhase.value = STATE_MAP[today.state] || 'work'
  }

  async function command(action: () => Promise<unknown>) {
    try {
      await action()
      await refetch()
    } catch (error) {
      console.error('Pomodoro command failed:', error)
    }
  }

  function startTimer() {
    return command(() => startPomodoroMutation({ durationSec: timeLeft.value }))
  }

  function pauseTimer() {
    return command(() => pausePomodoroMutation())
  }

  const resumeTimer = startTimer

  function stopTimer() {
    return command(() => stopPomodoroMutation())
  }

  function handleClick(event: MouseEvent) {
    if (totalTime.value === 0) return
    event.preventDefault()
    const svg = event.currentTarget as SVGElement
    const rect = svg.getBoundingClientRect()
    const centerX = rect.left + rect.width / 2
    const centerY = rect.top + rect.height / 2
    let angle = Math.atan2(event.clientX - centerX, -(event.clientY - centerY))
    if (angle < 0) angle += 2 * Math.PI
    const elapsedTime = Math.round((angle / (2 * Math.PI)) * totalTime.value)
    const seconds = Math.max(1, totalTime.value - elapsedTime)
    void command(() => startPomodoroMutation({ durationSec: seconds }))
  }

  function showNotification(completedState: string) {
    const title = completedState === 'WORK' ? t('work_completed') : t('break_completed')
    const body = completedState === 'WORK' ? t('time_for_break') : t('time_for_work')
    showDesktopNotification({ title, body, icon: '/favicon.ico' }).catch(() => {
      toast(`${title} - ${body}`)
    })
  }

  function handlePomodoroAction(data: any) {
    if (!data?.today) return
    applySnapshot(data.today)
    if (!data.completedState) return
    if (settings.value.playSoundOnComplete) playNotificationSound()
    if (settings.value.showNotification) {
      showNotification(data.completedState)
    } else {
      const message = currentPhase.value === 'longBreak' ? 'long_break_time'
        : currentPhase.value === 'shortBreak' ? 'short_break_time' : 'work_time_start'
      toast(t(message))
    }
  }

  function handlePomodoroSettingsUpdate() {
    void refetch()
  }

  function handleConnectionChanged(connected: boolean) {
    if (connected) void refetch()
  }

  const { refetch } = initQuery({
    document: pomodoroTodayAndSettingsGQL,
    handle: (data: any, error: string) => {
      if (error || !data) return
      if (data.pomodoroSettings) settings.value = data.pomodoroSettings
      applySnapshot(data.pomodoroToday)
    },
  })

  const circumference = 2 * Math.PI * 110
  const strokeDashoffset = computed(() => {
    const progress = totalTime.value > 0 ? (totalTime.value - timeLeft.value) / totalTime.value : 0
    return circumference * (1 - progress)
  })

  function formatTime(seconds: number): string {
    const mins = Math.floor(seconds / 60)
    const secs = seconds % 60
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`
  }

  function getCurrentPhaseText(): string {
    const map = { work: 'work_time', shortBreak: 'short_break', longBreak: 'long_break' }
    return t(map[currentPhase.value] || '')
  }

  function requestNotificationPermission() {
    // Tauri (WKWebView) blocks Notification.requestPermission() outside a user
    // gesture. macOS native notifications do not require browser permission.
    if (__IS_TAURI__) return
    if ('Notification' in window && typeof Notification !== 'undefined' && Notification.permission === 'default') {
      Notification.requestPermission()
    }
  }

  onMounted(() => {
    requestNotificationPermission()
    emitter.on('POMODORO_ACTION', handlePomodoroAction)
    emitter.on('POMODORO_SETTINGS_UPDATE', handlePomodoroSettingsUpdate)
    emitter.on('app_socket_connection_changed', handleConnectionChanged)
  })

  onUnmounted(() => {
    emitter.off('POMODORO_ACTION', handlePomodoroAction)
    emitter.off('POMODORO_SETTINGS_UPDATE', handlePomodoroSettingsUpdate)
    emitter.off('app_socket_connection_changed', handleConnectionChanged)
  })

  return {
    settings,
    isRunning,
    isPaused,
    timeLeft,
    totalTime,
    currentPhase,
    currentRound,
    completedToday,
    circumference,
    strokeDashoffset,
    formatTime,
    getCurrentPhaseText,
    startTimer,
    pauseTimer,
    resumeTimer,
    stopTimer,
    handleClick,
  }
}
