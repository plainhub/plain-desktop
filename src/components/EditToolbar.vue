<template>
  <div class="top-app-bar">
    <ul class="v-tabs">
      <li v-for="(item, index) in tabs" :key="index" :class="{ active: currentTab === index }" @click="swtichTab(index)">
        {{ item.startsWith('t:') ? $t(item.slice(2)) : item }}
      </li>
    </ul>
    <div class="actions">
      <v-filled-button :loading="loading" @click="save">
        {{ $t('save') }}
      </v-filled-button>
    </div>
  </div>
</template>
<script setup lang="ts">
import { ref, type PropType } from 'vue'
const emit = defineEmits(['update:modelValue'])

const props = defineProps({
  modelValue: { type: Number, default: 0 },
  save: { type: Function as PropType<(payload: MouseEvent) => void>, required: true },
  loading: { type: Boolean },
  tabs: { type: Array as PropType<Array<string>>, default: () => [] },
})

const currentTab = ref(props.modelValue)

function swtichTab(tab: number) {
  currentTab.value = tab
  emit('update:modelValue', tab)
}
</script>
