<template>
  <div class="quick-content-main">
    <div class="top-app-bar">
      <button v-tooltip="$t('close')" class="btn-icon" @click.prevent="store.quick = ''">
        <i-material-symbols:arrow-back-rounded />
      </button>
      <div class="title">
        {{ $t('pomodoro_timer') }}
        <!-- Warning icon for notification permissions -->
        <div v-if="settings.showNotification && hasNotificationWarning" class="warning-indicator">
          <v-dropdown v-model="warnOpen">
            <template #trigger>
              <button class="btn-icon warning-icon">
                <i-material-symbols:warning-outline />
              </button>
            </template>
            <div class="warning-dropdown">
              <div class="warning-content">
                <i-material-symbols:error-outline-rounded />
                <div class="warning-text">
                  {{ $t(notificationWarningMessage) }}
                </div>
              </div>
              <div v-if="notificationWarningAction" class="warning-actions">
                <v-filled-button @click="notificationWarningAction.action()">
                  {{ $t(notificationWarningAction.text) }}
                </v-filled-button>
              </div>
            </div>
          </v-dropdown>
        </div>
      </div>
    </div>

    <div class="quick-content-body">
      <div class="timer-container">
        <div class="status-display">
          <div class="current-phase">
            {{ getCurrentPhaseText() }}
          </div>
          <div class="round-info">
            {{ $t('round_n_of_n', { current: currentRound, total: settings.pomodorosBeforeLongBreak }) }}
          </div>
        </div>

        <!-- Circular timer -->
        <div class="timer-circle">
          <svg class="circle-progress" :class="{ adjustable: totalTime > 0 }" width="240" height="240" viewBox="0 0 240 240" @click="totalTime > 0 ? handleClick($event) : null">
            <circle cx="120" cy="120" r="110" fill="none" stroke="var(--md-sys-color-surface-variant)" stroke-width="8" />
            <circle
              cx="120"
              cy="120"
              r="110"
              fill="none"
              stroke="var(--md-sys-color-primary)"
              stroke-width="8"
              stroke-linecap="round"
              :stroke-dasharray="circumference"
              :stroke-dashoffset="strokeDashoffset"
              transform="rotate(-90 120 120)"
              class="progress-ring"
            />
          </svg>
          <div class="timer-content">
            <div class="time-display">{{ formatTime(timeLeft) }}</div>
            <div v-if="totalTime > 0" class="click-hint">{{ $t('click_to_adjust') }}</div>
          </div>
        </div>

        <!-- Control buttons -->
        <div class="timer-controls">
          <v-filled-button v-if="!isRunning && !isPaused" class="btn-start" @click="startTimer">
            {{ $t('start') }}
          </v-filled-button>

          <v-filled-button v-if="isRunning" class="btn-pause" @click="pauseTimer">
            {{ $t('pause') }}
          </v-filled-button>

          <v-filled-button v-if="isPaused" class="btn-resume" @click="resumeTimer">
            {{ $t('resume') }}
          </v-filled-button>

          <v-filled-button v-if="isRunning" class="btn-stop" @click="stopTimer">
            {{ $t('stop') }}
          </v-filled-button>
        </div>

        <!-- Daily stats -->
        <div class="daily-stats">
          <div class="stats-title">{{ $t('today_completed') }}</div>
          <div class="tomato-display">
            <span v-for="i in Math.max(completedToday, 4)" :key="i" class="tomato-icon" :class="{ completed: i <= completedToday }"> 🍅 </span>
          </div>
          <div class="stats-number">{{ $t('x_pomodoros', { count: completedToday }) }}</div>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useMainStore } from '@/stores/main'
import { usePomodoro } from './pomodoro'
import { useNotificationWarning } from '@/hooks/notification-warning'

const store = useMainStore()
const warnOpen = ref(false)

const {
  settings,
  isRunning,
  isPaused,
  timeLeft,
  totalTime,
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
} = usePomodoro()

const {
  hasWarning: hasNotificationWarning,
  warningMessage: notificationWarningMessage,
  warningAction: notificationWarningAction,
} = useNotificationWarning({ showToast: true })
</script>

<style lang="scss" scoped>
.timer-container {
  padding: 32px 16px 16px 16px;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 24px;
}

.status-display {
  text-align: center;

  .current-phase {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    margin-bottom: 8px;
    font-size: 1.125rem;
    font-weight: 500;
    color: var(--md-sys-color-on-surface);
  }

  .round-info {
    font-size: 0.875rem;
    color: var(--md-sys-color-on-surface-variant);
  }
}

.timer-circle {
  position: relative;
  display: flex;
  align-items: center;
  justify-content: center;

  .circle-progress {
    &.adjustable {
      cursor: pointer;

      &:hover .progress-ring {
        stroke-width: 10;
      }
    }
  }

  .progress-ring {
    transition:
      stroke-dashoffset 0.3s ease,
      stroke-width 0.2s ease;
  }

  .timer-content {
    position: absolute;
    text-align: center;
    pointer-events: none;

    .time-display {
      font-size: 2.5rem;
      font-weight: bold;
      color: var(--md-sys-color-on-surface);
      font-family: 'Courier New', monospace;
    }

    .click-hint {
      font-size: 0.75rem;
      color: var(--md-sys-color-on-surface-variant);
      margin-top: 8px;
      opacity: 0.7;
    }
  }
}

.timer-controls {
  display: flex;
  gap: 12px;

  button {
    &.v-filled-button {
      min-width: 140px;
    }
    &.btn-start,
    &.btn-resume {
      background: var(--md-sys-color-primary);
      color: var(--md-sys-color-on-primary);
    }

    &.btn-pause {
      background: var(--md-sys-color-secondary);
      color: var(--md-sys-color-on-secondary);
    }

    &.btn-stop {
      background: var(--md-sys-color-error);
      color: var(--md-sys-color-on-error);
    }
  }
}

.daily-stats {
  text-align: center;
  padding: 16px;

  .stats-title {
    font-size: 0.875rem;
    color: var(--md-sys-color-on-surface-variant);
    margin-bottom: 8px;
  }

  .tomato-display {
    display: flex;
    justify-content: center;
    gap: 4px;
    margin-bottom: 8px;
    flex-wrap: wrap;

    .tomato-icon {
      font-size: 1.5rem;
      opacity: 0.3;
      transition: opacity 0.2s ease;

      &.completed {
        opacity: 1;
      }
    }
  }

  .stats-number {
    font-size: 1rem;
    font-weight: 500;
    color: var(--md-sys-color-on-surface);
  }
}

</style>
