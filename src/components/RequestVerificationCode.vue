<template>
  <div class="request-verification-code">
    <code>{{ code }}</code>
    <p>{{ hint }}</p>
  </div>
</template>

<script setup lang="ts">
defineProps<{ code: string; hint: string }>()
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
