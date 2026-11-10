<template>
  <div class="request-verification-code">
    <code>{{ displayCode }}</code>
    <p>{{ hint }}</p>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
const props = defineProps<{ code: string; hint: string }>()
const displayCode = computed(() => /^[0-9a-f]{64}$/i.test(props.code) ? props.code.match(/.{8}/g)!.join(' ') : props.code)
</script>

<style scoped>
.request-verification-code {
  padding: 16px;
  border-radius: 16px;
  background: var(--md-sys-color-primary-container);
  color: var(--md-sys-color-on-primary-container);
  text-align: center;
}
code {
  display: block;
  font-size: 1.5rem;
  font-variant-numeric: tabular-nums;
  overflow-wrap: anywhere;
}
p { margin: 8px 0 0; }
</style>
