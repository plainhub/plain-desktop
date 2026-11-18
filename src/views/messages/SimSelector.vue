<template>
  <v-dropdown v-model="dropdownOpen">
    <template #trigger>
      <button class="sim-btn" :aria-label="'SIM ' + currentSlot" @click.stop="dropdownOpen = !dropdownOpen">
        {{ currentSlot }}
      </button>
    </template>
    <div
      v-for="(sim, idx) in sims"
      :key="sim.subscriptionId"
      class="dropdown-item"
      :class="{ selected: sim.subscriptionId === modelValue }"
      @click="$emit('update:modelValue', sim.subscriptionId); dropdownOpen = false"
    >
      <i-material-symbols:sim-card-outline-rounded />
      <div class="sim-option__text">
        <span>SIM {{ idx + 1 }}{{ sim.label ? ' · ' + sim.label : '' }}</span>
        <span v-if="sim.number" class="sim-option__number">{{ sim.number }}</span>
      </div>
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import type { ISim } from '@/lib/interfaces'

const props = defineProps<{
  sims: ISim[]
  modelValue: number
}>()

defineEmits<{
  'update:modelValue': [value: number]
}>()

const dropdownOpen = ref(false)

const currentSlot = computed(() => {
  const idx = props.sims.findIndex((s) => s.subscriptionId === props.modelValue)
  if (idx >= 0) return idx + 1
  if (props.modelValue >= 0 && props.modelValue < props.sims.length) return props.modelValue + 1
  return 1
})
</script>

<style scoped lang="scss">
.sim-btn {
  width: 20px;
  height: 26px;
  clip-path: polygon(0 0, calc(100% - 6px) 0, 100% 6px, 100% 100%, 0 100%);
  background: var(--md-sys-color-primary);
  color: var(--md-sys-color-on-primary);
  border: none;
  cursor: pointer;
  font-size: 11px;
  font-weight: 700;
  display: flex;
  align-items: center;
  justify-content: center;

  &:hover {
    filter: brightness(0.85);
  }
}

.sim-option__text {
  display: flex;
  flex-direction: column;
  gap: 1px;
}

.sim-option__number {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
