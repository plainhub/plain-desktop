<template>
  <div class="scroll-content">
    <p class="subtle">{{ $t('media_exclusions.hint') }}</p>
    <p v-if="!ready && !error" class="subtle">{{ $t('loading') }}</p>
    <div v-if="ready" class="directories">
      <p v-if="!directories.length" class="subtle">{{ $t('media_exclusions.empty') }}</p>
      <div v-for="path in directories" :key="path" class="directory-row">
        <span>{{ path }}</span>
        <v-icon-button :disabled="saving" :aria-label="$t('remove')" @click="save(directories.filter((item) => item !== path))">
          <i-material-symbols:close-rounded />
        </v-icon-button>
      </div>
      <v-text-button :disabled="saving" @click="add">{{ $t('file_index.add_directory') }}</v-text-button>
    </div>
    <p v-if="error" role="alert" class="error">{{ error }}</p>
    <v-text-button v-if="!ready && error" @click="load">{{ $t('retry') }}</v-text-button>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { promptModal } from '@/components/modal'
import DirectoryPickerModal from '@/components/DirectoryPickerModal.vue'
import * as prefs from '@/lib/prefs/core'

const directories = computed(() => prefs.get<string[]>('media_excluded_dirs', []))
const ready = ref(false)
const saving = ref(false)
const error = ref('')

onMounted(load)

async function load() {
  error.value = ''
  try {
    await prefs.preload()
    ready.value = true
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause)
  }
}

async function save(paths: string[]) {
  saving.value = true
  error.value = ''
  try {
    await prefs.set('media_excluded_dirs', paths)
  } catch (cause) {
    error.value = cause instanceof Error ? cause.message : String(cause)
  } finally {
    saving.value = false
  }
}

async function add() {
  const path = await promptModal<string>(DirectoryPickerModal, {})
  if (typeof path === 'string' && path && !directories.value.includes(path)) {
    await save([...directories.value, path])
  }
}
</script>

<style scoped>
.scroll-content {
  flex: 1;
  overflow-y: auto;
  padding: 16px;
}
p {
  margin: 0 0 16px;
}
.directories {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.directory-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.directory-row span {
  flex: 1;
  min-width: 0;
  overflow-wrap: anywhere;
}
.error {
  color: var(--md-sys-color-error);
}
</style>
