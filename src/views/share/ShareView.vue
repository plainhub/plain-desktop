<template>
  <div class="share-page">
    <div class="top-app-bar share-top-bar">
      <div class="title">
        <v-icon-button v-if="currentPath" v-tooltip="$t('back')" @click="goUp">
          <i-material-symbols:arrow-back-rounded />
        </v-icon-button>
        <div class="breadcrumb">
          <a href="#" @click.prevent="navigateTo('')">{{ shareTitle }}</a>
          <template v-for="(crumb, i) in breadcrumbs" :key="i">
            <span class="sep">/</span>
            <a v-if="i < breadcrumbs.length - 1" href="#" @click.prevent="navigateTo(crumb.path)">{{ crumb.name }}</a>
            <span v-else class="crumb-current">{{ crumb.name }}</span>
          </template>
        </div>
        <span v-if="info?.readOnly" class="status-badge">{{ $t('share_read_only') }}</span>
      </div>
      <div v-if="info?.expiresAt" class="actions">
        <span class="expiry">{{ $t('share_expires_on', { date: formatDateTime(new Date(info.expiresAt).toISOString()) }) }}</span>
      </div>
    </div>

    <div class="share-body">
      <div v-if="state === 'loading'" class="share-center">
        <span class="spinner" />
      </div>

      <div v-else-if="state === 'error'" class="share-center share-message">
        <h3>{{ errorTitle }}</h3>
        <p>{{ errorTip }}</p>
        <v-outlined-button v-if="errorCode !== 'invalid_link'" @click="load(currentPath)">
          <i-material-symbols:refresh-rounded /> {{ $t('retry') }}
        </v-outlined-button>
      </div>

      <template v-else>
        <div v-if="entries.length === 0" class="share-center share-message">
          <h3>{{ $t('share_empty') }}</h3>
          <p>{{ $t('share_empty_tip') }}</p>
        </div>
        <div v-else class="file-list">
          <div v-for="entry in entries" :key="entry.relativePath" class="file-item" @click="onItemClick(entry)">
            <div class="image">
              <FileThumb
                :is-dir="entry.isDir"
                :thumb-url="entry.hasThumb ? thumbSrc(entry) : ''"
                :extension="entryExt(entry)"
                :thumb-error="thumbErrorIds.includes(entry.relativePath)"
                :ext-error="extErrorIds.includes(entryExt(entry))"
                :on-thumb-error="() => onThumbError(entry.relativePath)"
                :on-ext-error="() => onExtError(entryExt(entry))"
              />
            </div>
            <div class="title">
              {{ entry.name }}
              <OnlinePreviewIcon :name="entry.name" :is-dir="entry.isDir" />
            </div>
            <div class="subtitle">{{ entry.isDir ? $t('folder') : formatFileSize(entry.size) }}</div>
            <div class="actions">
              <v-icon-button v-tooltip="$t('download')" @click.stop="downloadEntry(entry)">
                <i-material-symbols:download-rounded />
              </v-icon-button>
            </div>
          </div>
        </div>
      </template>
    </div>

    <lightbox />
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import { useRoute } from 'vue-router'
import { formatFileSize, formatDateTime } from '@/lib/format'
import { useShareData } from './hooks/useShareData'
import { useShareNavigation } from './hooks/useShareNavigation'
import { useShareActions } from './hooks/useShareActions'

const route = useRoute()

const sharedId = computed(() => String(route.params.sharedId || ''))
const sharedToken = computed(() => route.hash.replace(/^#/, ''))

const { state, errorCode, info, entries, shareTitle, errorTitle, errorTip, load } = useShareData(sharedId, sharedToken)
const { currentPath, breadcrumbs, navigateTo, goUp } = useShareNavigation(load)
const { thumbErrorIds, extErrorIds, entryExt, thumbSrc, onThumbError, onExtError, downloadEntry, onItemClick } =
  useShareActions(sharedId, info, entries, navigateTo)

onMounted(() => load(currentPath.value))
watch(() => [sharedId.value, sharedToken.value], () => load(currentPath.value))
</script>

<style lang="scss" scoped>
.share-page {
  height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--md-sys-color-surface);
  color: var(--md-sys-color-on-surface);
  box-sizing: border-box;
}

.share-top-bar {
  flex-shrink: 0;

  .title {
    min-width: 0;
  }

  .breadcrumb {
    display: flex;
    align-items: center;
    overflow-x: auto;
    flex-wrap: nowrap;
    white-space: nowrap;
    scrollbar-width: none;

    &::-webkit-scrollbar {
      display: none;
    }

    .sep {
      margin-inline: 4px;
      color: var(--md-sys-color-on-surface-variant);
    }

    a {
      color: var(--md-sys-color-primary);
      text-decoration: none;
    }

    .crumb-current {
      color: var(--md-sys-color-on-surface);
      font-weight: 500;
    }
  }

  .status-badge {
    flex-shrink: 0;
  }

  .expiry {
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant);
  }
}

.share-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding-block-end: 24px;
}

.share-center {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  text-align: center;
  padding: 40px 16px;
  box-sizing: border-box;
  height: 100%;

  h3 {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 500;
  }

  p {
    margin: 0;
    color: var(--md-sys-color-on-surface-variant);
    font-size: 0.9rem;
    max-width: 420px;
  }
}

.spinner {
  width: 28px;
  height: 28px;
  border: 3px solid color-mix(in srgb, var(--md-sys-color-primary) 25%, transparent);
  border-top-color: var(--md-sys-color-primary);
  border-radius: 50%;
  animation: share-spin 0.8s linear infinite;
}

@keyframes share-spin {
  to {
    transform: rotate(360deg);
  }
}

.file-list {
  padding: 8px 0;
}

.file-item {
  display: grid;
  grid-template-areas:
    'image title actions'
    'image subtitle actions';
  grid-template-columns: 56px 1fr auto;
  align-items: center;
  gap: 2px 12px;
  margin: 0 16px 8px;
  padding: 8px 12px;
  border-radius: var(--pl-shape-m);
  box-sizing: border-box;
  background: var(--md-sys-color-surface-container-low);
  cursor: pointer;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  .image {
    grid-area: image;
    width: 56px;
    height: 56px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--pl-shape-m);
    overflow: hidden;
    background: var(--md-sys-color-surface-container);

    :deep(.svg) {
      width: 32px;
      height: 32px;
    }

    :deep(.image-thumb) {
      object-fit: cover;
      border-radius: 0;
    }
  }

  .title {
    grid-area: title;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .subtitle {
    grid-area: subtitle;
    font-size: 0.8rem;
    color: var(--md-sys-color-on-surface-variant);
  }

  .actions {
    grid-area: actions;
  }
}
</style>
