<template>
  <section class="index-directories">
    <h3>{{ title }}</h3>
    <p v-if="paths.length === 0">{{ $t('index_no_directories') }}</p>
    <div v-for="path in paths" :key="path" class="directory-row">
      <span>{{ path }}</span>
      <v-icon-button v-if="editable" :disabled="busy" :aria-label="$t('remove')" @click="$emit('remove', path)"><i-lucide:x /></v-icon-button>
    </div>
    <v-text-button v-if="editable" :disabled="busy" @click="$emit('add')">{{ $t('add') }}</v-text-button>
  </section>
</template>

<script setup lang="ts">
defineProps<{ title: string; paths: string[]; editable?: boolean; busy?: boolean }>()
defineEmits<{ add: []; remove: [path: string] }>()
</script>

<style scoped>
.index-directories { display: flex; flex-direction: column; gap: 8px; }
h3 { margin: 0; font-size: 1rem; }
p { margin: 0; color: var(--md-sys-color-on-surface-variant); }
.directory-row { display: flex; align-items: center; gap: 8px; }
.directory-row span { flex: 1; overflow-wrap: anywhere; }
</style>
