<template>
  <v-modal @close="$emit('close')">
    <template #headline>
      {{ $t('add_bookmarks') }}
    </template>
    <template #content>
        <div class="help-text">{{ $t('add_bookmarks_hint') }}</div>
        <textarea
          ref="textareaRef"
          v-model="urlsText"
          class="url-textarea"
          :placeholder="$t('add_bookmarks_placeholder')"
          rows="8"
        />
      <div v-if="groups.length" class="form-row">
        <v-select v-model="selectedGroupId" :label="$t('add_to_group')" :options="groupOptions" />
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="$emit('close')">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :loading="saving" :disabled="!validUrls.length" @click="save">
        {{ $t('save') }}{{ validUrls.length ? ` (${validUrls.length})` : '' }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { ref, computed, onMounted, nextTick } from 'vue'
import { useI18n } from 'vue-i18n'
import type { BookmarkGroup } from '@/stores/bookmarks'
import { initMutation } from '@/lib/api/mutation'
import { addBookmarksGQL } from '@/lib/api/mutation'

const props = defineProps<{
  defaultGroupId: string
  groups: BookmarkGroup[]
}>()

const emit = defineEmits<{
  close: []
  saved: [bookmarks: any[]]
}>()

const { t } = useI18n()
const textareaRef = ref<HTMLTextAreaElement>()
const urlsText = ref('')
const selectedGroupId = ref(props.defaultGroupId)

const groupOptions = computed(() => [
  { value: '', label: t('ungrouped') },
  ...props.groups.map((g) => ({ value: g.id, label: g.name })),
])

const validUrls = computed(() => {
  return urlsText.value
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => l.length > 0 && (l.startsWith('http://') || l.startsWith('https://')))
})

const { mutate: mutateAdd, onDone: onAddDone, loading: saving } = initMutation({
  document: addBookmarksGQL,
}, showActionError)

onAddDone((r: any) => {
  if (r.data?.addBookmarks) {
    emit('saved', r.data.addBookmarks.map((b: any) => ({ ...b })))
  }
})

onMounted(async () => {
  await nextTick()
  setTimeout(() => textareaRef.value?.focus(), 100)
})

function save() {
  if (!validUrls.value.length) return
  mutateAdd({ urls: validUrls.value, groupId: selectedGroupId.value })
}
</script>

<style scoped lang="scss">
.url-textarea {
  width: 100%;
  padding: 10px 12px;
  border-radius: 8px;
  border: 1px solid var(--md-sys-color-outline);
  background: var(--md-sys-color-surface-container-highest);
  color: var(--md-sys-color-on-surface);
  font-size: 0.875rem;
  font-family: monospace;
  resize: vertical;
  outline: none;
  box-sizing: border-box;

  &:focus {
    border-color: var(--md-sys-color-primary);
  }

  &::placeholder {
    color: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
  }
}
</style>
