<!-- eslint-disable vue/no-v-html -->
<template>
  <div v-if="showDate" class="chat-date">{{ formatDate(data.createdAt) }}</div>
  <div ref="itemRef" class="chat-item" :class="{ self: isSelf }" @contextmenu="onContextMenu">
    <div class="chat-title">
      <v-dropdown v-model="rawOpen">
        <template #trigger>
          <span class="name">{{ senderName }}</span>
        </template>
        <pre class="view-raw">{{ data }}</pre>
      </v-dropdown>
      <time v-tooltip="formatDateTimeFull(data.createdAt)" class="time">{{ formatTime(data.createdAt) }}</time>
      <span v-if="showSending" class="sending">{{ sendingStatus }}</span>
      <span
        v-else-if="data.fromId === 'me' && (data.status === ChatStatus.FAILED || data.status === ChatStatus.PARTIAL)"
        :id="errorAnchorId"
        class="btn-icon send-error"
        @click.prevent.stop="showErrorMenu"
      >
        <i-material-symbols:warning-rounded />
      </span>
      <button class="btn-icon more-btn" type="button" @click.stop="openMoreMenu">
        <i-material-symbols:more-horiz aria-hidden="true" />
      </button>
    </div>
    <v-dropdown-menu v-model="statusMenuOpen" :anchor="errorAnchorId">
      <div class="status-header">
        <i-material-symbols:error-outline-rounded class="status-header-icon" />
        <span>{{ $t('delivery_status') }}</span>
      </div>
      <div v-if="data.channelId && deliveryResults.length > 0" class="status-body">
        <div v-if="deliveryResults.length > 1" class="status-summary">
          {{ $t('delivery_status_summary', { delivered: deliveredCount, total: deliveryResults.length }) }}
        </div>
        <div v-for="r in deliveryResults" :key="r.peerId" class="status-peer">
          <i-material-symbols:check-rounded v-if="!r.error" class="status-icon ok" />
          <i-material-symbols:error-outline-rounded v-else class="status-icon fail" />
          <span class="status-peer-name">{{ r.peerName }}</span>
          <span v-if="r.error" class="status-peer-error">{{ r.error }}</span>
        </div>
      </div>
      <div v-else class="status-error">{{ errorMessage }}</div>
      <div class="dropdown-item" @click="doRetry">
        <i-lucide:rotate-ccw class="status-retry-icon" />
        {{ $t('try_again') }}
      </div>
    </v-dropdown-menu>
    <div class="chat-content">
      <div v-if="data._content.type === MessageType.TEXT">
        <pre v-html="addLinksToURLs(data._content.value.text)"></pre>
        <ChatLinkPreviews v-if="data._content.value.linkPreviews?.length" :data="data" />
      </div>
      <component :is="getComponent(data._content.type)" v-else :data="data" :download-info="downloadInfo" :peer="peer" @download-action="(a: 'pause' | 'resume' | 'retry') => emit('download-action', data.id, a)" />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { formatTime, formatDateTimeFull, formatDate } from '@/lib/format'
import { addLinksToURLs } from '@/lib/strutil'
import { selectedTextWithin } from '@/lib/dom'
import { copyTextToClipboard } from '@/lib/clipboard'
import ILucideCopy from '~icons/lucide/copy'
import ILucideForward from '~icons/lucide/forward'
import ILucideTrash2 from '~icons/lucide/trash-2'
import { contextmenu } from '@/components/contextmenu'
import type { MenuItem } from '@/components/contextmenu/ContextMenuDefine'
import { useRevealFile } from './hooks/reveal-file'
import ChatImages from './ChatImages.vue'
import ChatLinkPreviews from './ChatLinkPreviews.vue'
import ChatFiles from './ChatFiles.vue'
import type { IChatItem, IPeer } from '@/lib/interfaces'
import { ChatStatus, MessageType } from '@/lib/status'

const props = defineProps<{
  data: IChatItem
  showDate: boolean
  senderName: string
  sendingStatus: string
  deleteLoading: boolean
  downloadInfo: { downloaded: number; total: number; speed: number; status: string } | null
  peer: IPeer | null
}>()

const emit = defineEmits<{ delete: [id: string]; forward: [item: IChatItem]; retry: [id: string, statusData?: string]; 'download-action': [id: string, action: 'pause' | 'resume' | 'retry'] }>()
const statusMenuOpen = ref(false)
const rawOpen = ref(false)
const { t } = useI18n()
const { buildMediaMenuItems } = useRevealFile()

const itemRef = ref<HTMLElement | null>(null)
let copySelection = ''

function captureCopySelection() {
  copySelection = selectedTextWithin(itemRef.value)
}

const isSelf = computed(() => props.data.fromId === 'me')
const errorAnchorId = computed(() => `msg-error-${props.data.id}`)

const showSending = computed(() => {
  if (props.data.fromId !== 'me') return false
  if (props.data.status === ChatStatus.PENDING) return true
  return props.data.id.startsWith('new_') && props.data.status !== ChatStatus.FAILED
})

const isText = computed(() => props.data._content.type === MessageType.TEXT)

function copyText() {
  const text = copySelection || (props.data._content.value.text ?? '')
  if (text) copyTextToClipboard(text)
}

function deleteEntry(divided: boolean): MenuItem {
  return {
    label: String(t('delete_message')),
    icon: ILucideTrash2,
    customClass: 'danger',
    divided,
    disabled: props.deleteLoading || showSending.value,
    onClick: () => emit('delete', props.data.id),
  }
}

const menuItems = computed<MenuItem[]>(() => {
  const items: MenuItem[] = []
  if (isText.value) {
    items.push({ label: String(t('copy_text')), icon: ILucideCopy, onClick: copyText })
  }
  if (!props.data.id.startsWith('new_')) {
    items.push({ label: String(t('forward_message')), icon: ILucideForward, onClick: () => emit('forward', props.data) })
  }
  items.push(deleteEntry(items.length > 0))
  return items
})

function onContextMenu(e: MouseEvent) {
  if (e.defaultPrevented) return
  e.preventDefault()
  captureCopySelection()
  const el = (e.target as HTMLElement).closest('[data-ctx="media"]')
  const mediaItems = el ? buildMediaMenuItems(el.getAttribute('data-path') ?? '', el.getAttribute('data-name') ?? '') : []
  const items = mediaItems.length > 0 ? [...mediaItems, deleteEntry(true)] : [...menuItems.value]
  contextmenu({ x: e.clientX, y: e.clientY, items })
}

function openMoreMenu(e: MouseEvent) {
  captureCopySelection()
  const r = (e.currentTarget as HTMLElement).getBoundingClientRect()
  contextmenu({ x: r.right, y: r.bottom + 4, items: menuItems.value })
}

interface DeliveryResult {
  peerId: string
  peerName: string
  error?: string | null
}

const deliveryResults = computed<DeliveryResult[]>(() => {
  if (!props.data.statusData) return []
  try {
    const sd = JSON.parse(props.data.statusData) as { results?: DeliveryResult[] }
    return sd.results ?? []
  } catch {
    return []
  }
})

const deliveredCount = computed(() => deliveryResults.value.filter((r) => !r.error).length)
const errorMessage = computed(() => deliveryResults.value[0]?.error || t('send_failed'))

function showErrorMenu() {
  document.dispatchEvent(new CustomEvent('dropdown-toggle', { detail: { exclude: document.getElementById(errorAnchorId.value) } }))
  statusMenuOpen.value = true
}

function doRetry() {
  statusMenuOpen.value = false
  emit('retry', props.data.id, props.data.statusData)
}

const componentMap: Record<string, any> = { [MessageType.IMAGES]: ChatImages, [MessageType.FILES]: ChatFiles, linkPreviews: ChatLinkPreviews }
function getComponent(type: string) { return componentMap[type] }
</script>
