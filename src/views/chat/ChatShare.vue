<template>
  <div class="share-card" :class="{ expired: isExpired }" @click="open">
    <div class="icon-box">
      <i-material-symbols:folder-shared-rounded />
    </div>
    <div class="share-info">
      <div class="share-name">{{ share.name }}</div>
      <div class="share-meta">
        <span class="meta-text">{{ subtitle }}</span>
        <span v-if="isExpired" class="expired-badge">{{ t('share_expired') }}</span>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useI18n } from 'vue-i18n'
import { openUrl } from '@/lib/browser'
import { formatFileSize, formatDateTime } from '@/lib/format'

const props = defineProps({
  data: { type: Object, required: true },
  downloadInfo: { type: Object as () => { downloaded: number; total: number; speed: number; status: string } | null, default: null },
  peer: { type: Object as () => { ip: string; port: number } | null, default: null },
})
const { t } = useI18n()

interface ISharePeerInfo {
  id: string
  ip: string
  port: number
}

interface IShareValue {
  shareId: string
  urlToken: string
  peerInfo: ISharePeerInfo
  name: string
  itemCount?: number
  totalSize?: number
  expiresAt?: string | number | null
}

const share = computed<IShareValue>(() => props.data._content?.value ?? ({} as IShareValue))

function expiryMs(expiresAt: IShareValue['expiresAt']): number | null {
  if (expiresAt == null || expiresAt === '') return null
  const ms = typeof expiresAt === 'number' ? expiresAt : new Date(expiresAt).getTime()
  return Number.isFinite(ms) ? ms : null
}

const expiry = computed(() => expiryMs(share.value.expiresAt))
const isExpired = computed(() => expiry.value != null && expiry.value <= Date.now())

const subtitle = computed(() => {
  const parts: string[] = [t('folder_card_items', { n: share.value.itemCount ?? 0 })]
  if (share.value.totalSize && share.value.totalSize > 0) {
    parts.push(formatFileSize(share.value.totalSize))
  }
  if (expiry.value != null && !isExpired.value) {
    parts.push(t('share_expires_on', { date: formatDateTime(new Date(expiry.value).toISOString()) }))
  }
  return parts.join(' · ')
})

function open() {
  const { shareId, urlToken, peerInfo } = share.value
  if (!shareId || !urlToken || !peerInfo?.ip || !peerInfo?.port) return
  openUrl(`https://${peerInfo.ip}:${peerInfo.port}/s/${shareId}#${urlToken}`)
}
</script>

<style lang="scss" scoped>
.share-card {
  display: flex;
  align-items: center;
  width: 320px;
  max-width: 100%;
  padding: 12px;
  background: var(--md-sys-color-surface-container);
  border-radius: 12px;
  cursor: pointer;
  transition: background 0.15s;
  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }
  &.expired {
    opacity: 0.62;
  }
}

.icon-box {
  display: flex;
  align-items: center;
  justify-content: center;
  width: 44px;
  height: 44px;
  border-radius: 12px;
  background: var(--md-sys-color-surface-container-high);
  color: var(--md-sys-color-primary);
  flex-shrink: 0;
  svg {
    width: 24px;
    height: 24px;
  }
}

.expired .icon-box {
  opacity: 0.62;
}

.share-info {
  flex: 1;
  min-width: 0;
  margin-left: 12px;
}

.share-name {
  font-weight: 500;
  color: var(--md-sys-color-on-surface);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.share-meta {
  display: flex;
  align-items: center;
  margin-top: 4px;
}

.meta-text {
  font-size: 0.875rem;
  color: var(--md-sys-color-on-surface-variant);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.expired-badge {
  margin-left: 8px;
  padding: 0 8px;
  border-radius: 8px;
  font-size: 0.75rem;
  line-height: 16px;
  background: var(--md-sys-color-error-container);
  color: var(--md-sys-color-on-error-container);
  flex-shrink: 0;
}
</style>
