<template>
  <v-modal @close="finish">
    <template #headline>
      {{ $t('review_mode') }}
    </template>
    <template #content>
      <div v-if="current" class="review-card">
        <div class="review-meta">
          {{ current.address }} · {{ formatDateTime(current.date) }}
        </div>
        <div class="review-body">{{ current.body }}</div>
        <div class="review-count">
          {{ $t('review_remaining', { count: remaining.length, trashed: trashedStack.length }) }}
        </div>
        <label v-if="deleteAvailable" class="review-permanent">
          <input v-model="permanent" type="checkbox" />
          <span>{{ $t('permanent_delete') }}</span>
        </label>
        <div v-if="permanent" class="review-hints">{{ $t('permanent_delete_hint') }}</div>
        <div v-else class="review-hints">{{ $t('review_hints') }}</div>
      </div>
      <div v-else class="review-empty">{{ $t('review_all_done') }}</div>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { Modal, popModal, promptModal } from '@/components/modal'
import OrganizeConfirmModal from '@/components/lightbox/OrganizeConfirmModal.vue'
import { useSmsTrash, useSmsRestore } from '@/hooks/sms-trash'
import { useSmsDeleteAvailable, useSmsDelete } from '@/hooks/sms-delete'
import { formatDateTime } from '@/lib/format'
import type { IMessage } from '@/lib/interfaces'

const props = defineProps<{
  items: IMessage[]
}>()

const emit = defineEmits<{
  (e: typeof Modal.EVENT_PROMPT, changed: boolean): void
}>()

const smsTrash = useSmsTrash()
const smsRestore = useSmsRestore()
const smsDelete = useSmsDelete()
const { available: deleteAvailable } = useSmsDeleteAvailable()

const remaining = ref([...props.items])
const trashedStack = ref<IMessage[]>([])
const deletedStack = ref<IMessage[]>([])
const permanent = ref(false)
const finished = ref(false)

const current = computed(() => remaining.value[0])

function trashCurrent() {
  const item = current.value
  if (!item) return
  if (permanent.value) {
    smsDelete.delete(`ids:${item.id}`)
    deletedStack.value.push(item)
  } else {
    smsTrash.trash(`ids:${item.id}`)
    trashedStack.value.push(item)
  }
  remaining.value.shift()
  if (!remaining.value.length) finish()
}

function keepCurrent() {
  if (!current.value) return
  remaining.value.shift()
  if (!remaining.value.length) finish()
}

function undoLast() {
  // Permanent deletes are irrevocable — only app-side trash can be undone.
  const item = trashedStack.value.pop()
  if (!item) return
  smsRestore.restore(`ids:${item.id}`)
  remaining.value.unshift(item)
}

async function finish() {
  if (finished.value) return
  finished.value = true
  const changed = trashedStack.value.length
  if (changed > 0) {
    const undo = await promptModal<boolean>(OrganizeConfirmModal, { count: changed })
    if (undo) {
      smsRestore.restore(`ids:${trashedStack.value.map((i) => i.id).join(',')}`)
    }
  }
  emit(Modal.EVENT_PROMPT, changed > 0 || deletedStack.value.length > 0)
  popModal()
}

function onKeydown(e: KeyboardEvent) {
  const key = e.key.toLowerCase()
  if (key === 'd' || key === 'x') {
    e.preventDefault()
    trashCurrent()
  } else if (key === 'k' || key === 'arrowright') {
    e.preventDefault()
    keepCurrent()
  } else if (key === 'z') {
    e.preventDefault()
    undoLast()
  }
  // Escape is handled by v-modal's close → finish()
}

onMounted(() => window.addEventListener('keydown', onKeydown))
onUnmounted(() => window.removeEventListener('keydown', onKeydown))
</script>

<style lang="scss" scoped>
.review-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  min-width: 320px;
}

.review-meta {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}

.review-body {
  font-size: 1.0625rem;
  line-height: 1.6;
  white-space: pre-wrap;
  word-break: break-word;
  max-height: 40vh;
  overflow-y: auto;
}

.review-count {
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
}

.review-hints {
  font-size: 0.75rem;
  color: var(--md-sys-color-outline);
}

.review-permanent {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 0.8125rem;
  color: var(--md-sys-color-on-surface);
  cursor: pointer;

  input[type='checkbox'] {
    accent-color: var(--md-sys-color-error);
  }
}

.review-empty {
  min-width: 240px;
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
}
</style>
