import { ref, computed, onMounted, onUnmounted, watch } from 'vue'
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

  // Mutations
  const { mutate: startPomodoroMutation } = initMutation({ document: startPomodoroGQL })
  const { mutate: stopPomodoroMutation } = initMutation({ document: stopPomodoroGQL })
  const { mutate: pausePomodoroMutation } = initMutation({ document: pausePomodoroGQL })

  // State
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

  let timer: ReturnType<typeof setInterval> | null = null

  // Timer core
  function initTimer(phase: 'work' | 'shortBreak' | 'longBreak') {
    currentPhase.value = phase
    const durations = {
      work: settings.value.workDurationMin,
      shortBreak: settings.value.shortBreakDurationMin,
      longBreak: settings.value.longBreakDurationMin,
    }
    timeLeft.value = durations[phase] * 60
    totalTime.value = durations[phase] * 60
  }

  function setTimerState(running: boolean, paused: boolean) {
    isRunning.value = running
    isPaused.value = paused
    if (!running) clearTimer()
  }

  function clearTimer() {
    if (timer) {
      clearInterval(timer)
      timer = null
    }
  }

  function startTimerInterval() {
    clearTimer()
    timer = setInterval(() => {
      if (timeLeft.value > 0) {
        timeLeft.value--
      } else {
        timerComplete()
      }
    }, 1000)
  }

  function startTimer() {
    if (timeLeft.value === 0) initTimer('work')
    setTimerState(true, false)
    startPomodoroMutation({ timeLeftSec: timeLeft.value }).catch((e) => console.error('Failed to start pomodoro:', e))
    startTimerInterval()
  }

  function pauseTimer() {
    setTimerState(false, true)
    pausePomodoroMutation().catch((e) => console.error('Failed to pause pomodoro:', e))
  }

  const resumeTimer = startTimer

  function stopTimer() {
    setTimerState(false, false)
    stopPomodoroMutation().catch((e) => console.error('Failed to stop pomodoro:', e))
    initTimer(currentPhase.value)
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
    timeLeft.value = Math.max(0, totalTime.value - elapsedTime)
    setTimerState(true, false)
    startPomodoroMutation({ timeLeftSec: timeLeft.value }).catch((e) => console.error('Failed to update pomodoro progress:', e))
    startTimerInterval()
  }

  function showNotification() {
    const title = currentPhase.value === 'work' ? t('work_completed') : t('break_completed')
    const body = currentPhase.value === 'work' ? t('time_for_break') : t('time_for_work')
    showDesktopNotification({ title, body, icon: '/favicon.ico' }).catch(() => {
      toast(`${title} - ${body}`)
    })
  }

  function timerComplete() {
    setTimerState(false, false)
    if (settings.value.playSoundOnComplete) playNotificationSound()
    if (settings.value.showNotification) showNotification()

    if (currentPhase.value === 'work') {
      completedToday.value++
      if (currentRound.value >= settings.value.pomodorosBeforeLongBreak) {
        initTimer('longBreak')
        currentRound.value = 1
        if (!settings.value.showNotification) toast(t('long_break_time'))
      } else {
        initTimer('shortBreak')
        currentRound.value++
        if (!settings.value.showNotification) toast(t('short_break_time'))
      }
    } else {
      initTimer('work')
      if (!settings.value.showNotification) toast(t('work_time_start'))
    }
  }

  // WebSocket handlers
  function handlePomodoroAction(data: any) {
    if (!data) return
    if (data.timeLeftSec !== undefined) timeLeft.value = data.timeLeftSec
    if (data.totalTimeSec !== undefined) totalTime.value = data.totalTimeSec
    if (data.completedCount !== undefined) completedToday.value = data.completedCount
    if (data.round !== undefined) currentRound.value = data.round
    if (data.state !== undefined) currentPhase.value = STATE_MAP[data.state] || 'work'

    switch (data.action) {
      case 'start':
        isRunning.value = true
        isPaused.value = false
        if (timeLeft.value > 0) startTimerInterval()
        break
      case 'pause':
        isRunning.value = false
        isPaused.value = true
        clearTimer()
        break
      case 'stop':
        isRunning.value = false
        isPaused.value = false
        clearTimer()
        break
    }
  }

  function handlePomodoroSettingsUpdate(data: any) {
    if (!data) return
    settings.value = { ...settings.value, ...data }
    if (!isRunning.value && !isPaused.value && timeLeft.value === totalTime.value) {
      initTimer(currentPhase.value)
    }
  }

  // Data loading
  initQuery({
    document: pomodoroTodayAndSettingsGQL,
    handle: (data: any, error: string) => {
      if (error) {
        return
      }
      if (!data) return

      const { pomodoroSettings, pomodoroToday } = data
      if (pomodoroSettings) settings.value = pomodoroSettings

      if (pomodoroToday && pomodoroToday.totalTimeSec > 0) {
        completedToday.value = pomodoroToday.completedCount || 0
        currentRound.value = pomodoroToday.currentRound || 1
        timeLeft.value = pomodoroToday.timeLeftSec || 0
        totalTime.value = pomodoroToday.totalTimeSec
        isRunning.value = pomodoroToday.isRunning || false
        isPaused.value = pomodoroToday.isPaused || false
        if (pomodoroToday.state) currentPhase.value = STATE_MAP[pomodoroToday.state] || 'work'
        if (isRunning.value && !isPaused.value && timeLeft.value > 0) startTimerInterval()
      }
    },
  })

  // Computed
  const circumference = 2 * Math.PI * 110
  const strokeDashoffset = computed(() => {
    const progress = totalTime.value > 0 ? (totalTime.value - timeLeft.value) / totalTime.value : 0
    return circumference * (1 - progress)
  })

  // Helpers
  function formatTime(seconds: number): string {
    const mins = Math.floor(seconds / 60)
    const secs = seconds % 60
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`
  }

  function getCurrentPhaseText(): string {
    const map = { work: 'work_time', shortBreak: 'short_break', longBreak: 'long_break' }
    return t(map[currentPhase.value] || '')
  }

  // Lifecycle
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
    if (totalTime.value === 0) initTimer('work')
    emitter.on('pomodoro_action', handlePomodoroAction)
    emitter.on('pomodoro_settings_update', handlePomodoroSettingsUpdate)
  })

  onUnmounted(() => {
    clearTimer()
    emitter.off('pomodoro_action', handlePomodoroAction)
    emitter.off('pomodoro_settings_update', handlePomodoroSettingsUpdate)
  })

  watch(settings, () => {
    if (!isRunning.value && !isPaused.value && timeLeft.value === 0) initTimer('work')
  }, { deep: true })

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
