<template>
  <Teleport to="body">
    <div v-if="isActive" :class="['v-toast', type]" role="alert" @mouseover="toggleTimer(true)" @mouseleave="toggleTimer(false)" @click="click">
      <span>{{ message }}</span>
      <button v-if="actionLabel" class="v-toast-action" @click.stop="onActionClick">{{ actionLabel }}</button>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import Timer from './timer'

const props = defineProps({
  message: {
    type: String,
    required: true,
  },
  type: {
    type: String,
    default: '',
  },
  duration: {
    type: [Number, Boolean],
    default: 3000,
  },
  actionLabel: {
    type: String,
    default: '',
  },
  onAction: {
    type: Function,
    default: () => {},
  },
  onClick: {
    type: Function,
    default: () => {},
  },
})

const isActive = ref(true)

const timer = props.duration !== false ? new Timer(close, props.duration as number) : null

function click() {
  props.onClick.apply(null, arguments)
  close()
}

function onActionClick() {
  props.onAction.apply(null, arguments)
  close()
}

function toggleTimer(newVal: boolean) {
  if (timer) {
    newVal ? timer.pause() : timer.resume()
  }
}

function stopTimer() {
  timer && timer.stop()
}

function close() {
  stopTimer()
  isActive.value = false
}
</script>

<style lang="scss" scoped>
.v-toast {
  display: flex;
  align-items: center;
  gap: 12px;
}

.v-toast-action {
  border: none;
  background: none;
  color: var(--md-sys-color-primary);
  font-weight: 600;
  cursor: pointer;
  padding: 4px 8px;
  text-transform: uppercase;
  font-size: 0.9em;

  &:hover {
    text-decoration: underline;
  }
}
</style>
