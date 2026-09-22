<template>
  <v-modal @close="popModal">
    <template #headline>
      {{ $t('disk_manager') }}
      <v-icon-button v-tooltip="$t('refresh')" class="sm" :disabled="loading" @click.stop="refetch">
        <i-material-symbols:refresh-rounded />
      </v-icon-button>
    </template>

    <template #content>
      <div class="dm">
        <div v-if="errorText" class="dm-error">{{ errorText }}</div>
        <div v-else-if="loading" class="dm-loading">{{ $t('loading') }}</div>

        <template v-else>
          <div v-if="!sortedDisks.length" class="empty">{{ $t('no_disks') }}</div>

          <section v-for="(disk, idx) in sortedDisks" :key="disk.id" class="card border-card">
            <h5 class="card-title dm-card-title">
              <span class="dm-card-title-main">
                <span class="number">{{ idx + 1 }}.</span>
                <span class="dm-title">{{ diskTitle(disk, idx) }}</span>
                <span class="dm-size">({{ formatFileSize(Number(disk.sizeBytes || 0)) }})</span>
                <v-dropdown
                  :model-value="helpOpenId === disk.id"
                  @update:model-value="helpOpenId = helpOpenId === disk.id ? '' : disk.id"
                >
                  <template #trigger>
                    <v-icon-button class="btn-help sm">
                      <i-material-symbols:help-outline-rounded />
                    </v-icon-button>
                  </template>
                  <div class="dm-help-pop">
                    <div class="title">{{ diskSummaryText(disk) }}</div>
                    <dl class="meta">
                      <dt>{{ $t('unavailable_space') }}</dt>
                      <dd>
                        {{ formatFileSize(unavailableBytes(disk)) }}
                      </dd>
                      <template v-if="disk.model">
                        <dt>{{ $t('device_model') }}</dt>
                        <dd>{{ disk.model }}</dd>
                      </template>
                    </dl>
                    <div class="hint">{{ unavailableReasonText(disk) }}</div>
                  </div>
                </v-dropdown>

                <span v-if="disk.removable" class="dm-chip">{{ $t('removable') }}</span>
              </span>
              <v-outlined-button
v-if="canFormatDisk(disk)" class="sm dm-format" :disabled="loading"
                @click.stop="formatDisk(disk)">
                {{ $t('format_disk') }}
              </v-outlined-button>
            </h5>

            <div class="card-body">
              <template v-if="diskVolumes(disk).length">
                <div class="browser-volumes dm-volumes">
                  <div class="volumes">
                    <VolumeCard
v-for="v in diskVolumes(disk)" :key="v.id" :data="v"
                      :title="getStorageVolumeTitle(v, t)" :drive-type="String(v.driveType || '').trim()"
                      :used-percent="volumeUsedPercent(v)" :count="volumeCount(v)" :show-progress="true" />
                  </div>
                </div>
              </template>

              <div v-else class="hint dm-hint">
                {{ $t('disk_ready_hint') }}
              </div>
            </div>
          </section>
        </template>
      </div>
    </template>

    <template #actions>
      <v-outlined-button value="cancel" @click="popModal">{{ $t('close') }}</v-outlined-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useI18n } from 'vue-i18n'
import { initQuery, disksGQL, nasMountsMetaGQL } from '@/lib/api/query'
import type { IStorageDisk, IStorageMount } from '@/lib/interfaces'
import { formatFileSize, formatUsedTotalBytes } from '@/lib/format'
import VolumeCard from '@/components/storage/VolumeCard.vue'
import { getStorageVolumeTitle } from '@/lib/volumes'
import { popModal, pushModal } from '@/components/modal'
import FormatDiskConfirm from '@/components/storage/FormatDiskConfirm.vue'

const { t, locale } = useI18n()

const disks = ref<IStorageDisk[]>([])
const mounts = ref<IStorageMount[]>([])
const errorText = ref('')
const helpOpenId = ref('')

const disksQuery = initQuery<{ disks: IStorageDisk[] }>({
  document: disksGQL,
  handle: (data, error) => {
    if (error) {
      errorText.value = t(error)
      disks.value = []
      return
    }
    errorText.value = ''
    disks.value = (data?.disks ?? []) as IStorageDisk[]
  },
})

const mountsQuery = initQuery<{ mounts: IStorageMount[] }>({
  document: nasMountsMetaGQL,
  handle: (data, error) => {
    if (error) {
      // Keep disks visible even if volumes fail.
      return
    }
    mounts.value = (data?.mounts ?? []) as IStorageMount[]
  },
})

const loading = computed(() => !!disksQuery.loading.value || !!mountsQuery.loading.value)

const diskTitleCollator = computed(() => new Intl.Collator(locale.value, { numeric: true, sensitivity: 'base' }))

const sortedDisks = computed(() => {
  const collator = diskTitleCollator.value
  return [...(disks.value || [])].sort((a, b) => {
    const aTitle = diskTitle(a).trim()
    const bTitle = diskTitle(b).trim()
    const cmp = collator.compare(aTitle, bTitle)
    if (cmp) return cmp
    return String(a.path || a.name || '').localeCompare(String(b.path || b.name || ''))
  })
})

function refetch() {
  disksQuery.refetch()
  mountsQuery.refetch()
}

function volumeUsedPercent(v: IStorageMount) {
  const total = Number(v.totalBytes || 0)
  const used = Number(v.usedBytes || 0)
  if (!total) return 0
  const pct = (used / total) * 100
  if (!Number.isFinite(pct)) return 0
  return Math.max(0, Math.min(100, pct))
}

function volumeCount(v: IStorageMount) {
  const total = Number(v.totalBytes || 0)
  const used = Number(v.usedBytes || 0)
  if (!total) return ''
  return formatUsedTotalBytes(used, total)
}

function diskVolumes(d: IStorageDisk): IStorageMount[] {
  const id = String(d?.id || '').trim()
  if (!id) return []
  return (mounts.value || []).filter((v) => !String(v?.path ?? '').trim() && !v.remote && String(v.diskId || '').trim() === id)
}

function diskSummaryText(d: IStorageDisk) {
  const vols = diskVolumes(d)
  if (vols.length) return t('volume_count_x', { n: vols.length })
  return t('no_mounted_volumes')
}

function unavailableBytes(d: IStorageDisk) {
  const diskSize = Number(d?.sizeBytes || 0)
  if (!diskSize) return 0

  const vols = diskVolumes(d)

  // With the simplified UI we only consider mounted user-visible volumes.
  // The remaining space is metadata, reserved space, or unmounted/unallocated.
  let usable = 0
  for (const v of vols) {
    usable += Number(v.totalBytes || 0)
  }

  const gap = diskSize - usable
  if (!Number.isFinite(gap) || gap <= 0) return 0
  return gap
}

function unavailableReasonText(d: IStorageDisk) {
  if (diskVolumes(d).length) return t('unavailable_reason_whole_disk')
  return t('unavailable_reason_unset')
}

function diskTitle(d: IStorageDisk, idx = 0) {
  const n = (d.name || '').trim()
  // Prefer model when present; fallback to a friendlier label than raw device name.
  const model = (d.model || '').trim()
  if (model) return model
  const num = Number.isFinite(idx) ? idx + 1 : 1
  if (n.startsWith('nvme')) return `${t('disk')} (NVMe)`
  if (n.startsWith('mmcblk0')) return t('internal_storage')
  if (n.startsWith('mmcblk')) return t('sdcard')
  if (d.removable) return t('usb_storage')
  return t('disk')
}

function isSystemDisk(d: IStorageDisk) {
  if (diskVolumes(d).some((v) => String(v.mountPoint || '').trim() === '/')) return true

  const rootVol = (mounts.value || []).find((v) => !String(v?.path ?? '').trim() && String(v.mountPoint || '').trim() === '/')
  if (!rootVol) return false

  return String(rootVol.diskId || '').trim() === String(d.id || '').trim()
}

function canFormatDisk(d: IStorageDisk) {
  return !isSystemDisk(d)
}

function formatDisk(d: IStorageDisk) {
  pushModal(FormatDiskConfirm, {
    path: d.path,
    label: diskTitle(d),
    onDone: () => refetch(),
  })
}
</script>

<style scoped>
.dm {
  min-width: min(960px, 92vw);
}

.dm-error {
  color: var(--md-sys-color-error);
}

.dm-loading {
  color: var(--md-sys-color-on-surface-variant);
}

.dm-card-title {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}

.dm-card-title-main {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex-wrap: wrap;
}

.dm-title {
  font-weight: 650;
  overflow: hidden;
}

.dm-size {
  color: var(--md-sys-color-on-surface-variant);
}

.dm-chip {
  font-size: 12px;
  padding: 2px 8px;
  border-radius: 999px;
  border: 1px solid var(--md-sys-color-outline-variant);
  color: var(--md-sys-color-on-surface-variant);
  white-space: nowrap;
}

.dm-volumes.browser-volumes {
  margin-top: 8px;
  padding: 0;
}

.dm-hint {
  margin-top: 6px;
}

.dm-format {
  white-space: nowrap;
}

.dm-help-pop {
  max-width: min(520px, 80vw);
  padding: 12px 14px;
  display: flex;
  flex-direction: column;
  gap: 8px;
  font-weight: normal;

  .title {
    font-weight: 650;
    color: var(--md-sys-color-on-surface);
  }

  .meta {
    margin: 0;
    display: grid;
    grid-template-columns: 140px 1fr;
    gap: 4px 12px;

    dt {
      color: var(--md-sys-color-on-surface-variant);
    }

    dd {
      margin: 0;
      color: var(--md-sys-color-on-surface);
    }
  }
}
</style>
