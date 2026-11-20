<template>
  <div class="link-previews">
    <div v-for="preview in linkPreviews" :key="preview.url" class="link-preview-item" @click="openLink(preview.url)">
      <v-dropdown
        class="preview-actions"
        :class="{ 'is-active': openMenu === preview.url || savingUrls.has(preview.url) }"
        :model-value="openMenu === preview.url"
        @update:model-value="openMenu = $event ? preview.url : ''"
      >
        <template #trigger>
          <v-icon-button type="button" :aria-label="$t('actions')" :aria-expanded="openMenu === preview.url" :loading="savingUrls.has(preview.url)">
            <i-material-symbols:more-horiz aria-hidden="true" />
          </v-icon-button>
        </template>
        <button type="button" class="dropdown-item bookmark-action" :disabled="savingUrls.has(preview.url)" @click.stop="addBookmark(preview.url)">
          {{ $t('add_to_bookmarks') }}
        </button>
      </v-dropdown>
      <div v-if="preview.src && preview.large" class="preview-image">
        <img 
          :src="preview.src"
          :alt="preview.title ?? ''"
          @error="onImageError(preview)"
        />
      </div>
      
      <div class="preview-content">
        <div v-if="preview.domain" class="preview-domain">
          {{ preview.domain.toUpperCase() }}
        </div>
        
        <h4 v-if="preview.title" class="preview-title">{{ preview.title }}</h4>
        <p v-if="preview.description" class="preview-description">{{ preview.description }}</p>
        
        <div class="preview-footer">
          <img 
            v-if="preview.src && !preview.large"
            :src="preview.src"
            class="preview-small-icon"
            alt=""
            @error="onImageError(preview)"
          />
          <span class="preview-site">{{ preview.siteName || preview.url }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { computed, reactive, ref } from 'vue'
import { getChatFileId, getFileUrl } from '@/lib/api/file'
import { addBookmarksGQL, initMutation } from '@/lib/api/mutation'
import { openUrl } from '@/lib/browser'
import type { IChatItem } from '@/lib/interfaces'
import { useTempStore } from '@/stores/temp'

const props = defineProps<{ data: IChatItem }>()
const tempStore = useTempStore()
const failed = reactive(new Set<string>())
const openMenu = ref('')
const savingUrls = reactive(new Set<string>())
const { mutate: saveBookmark } = initMutation({ document: addBookmarksGQL }, showActionError)

async function addBookmark(url: string) {
  if (savingUrls.has(url)) return
  openMenu.value = ''
  savingUrls.add(url)
  try {
    await saveBookmark({ urls: [url], groupId: '' })
  } finally {
    savingUrls.delete(url)
  }
}

const linkPreviews = computed(() => {
  if (props.data.content.__typename !== 'ChatMessageContent') return []
  return props.data.content.linkPreviews.map(preview => {
    const imageKey = JSON.stringify([preview.url, preview.image])
    const fileId = preview.image?.startsWith('fid:') && preview.image.length > 4 && !failed.has(imageKey)
      ? getChatFileId(tempStore.urlTokenKey, preview.image)
      : ''
    return {
      ...preview,
      imageKey,
      src: fileId ? getFileUrl(fileId) : '',
      large: preview.imageWidth >= 200 && preview.imageHeight >= 200,
    }
  })
})

function onImageError({ imageKey }: { imageKey: string }) {
  failed.add(imageKey)
}

function openLink(url: string) {
  openUrl(url)
}
</script>

<style lang="scss" scoped>
.bookmark-action {
  border: 0;
  background: transparent;
  color: inherit;
  width: 100%;
}

.link-previews {
  margin-top: 6px;
  max-width: 600px;
  
  .link-preview-item {
    position: relative;
    border-radius: 12px;
    overflow: hidden;
    cursor: pointer;
    margin-bottom: 6px;
    background-color: var(--md-sys-color-surface-container);
    transition: background-color 0.2s ease;
    
    &:hover {
      background-color: var(--md-sys-color-surface-container-high);
    }
    
    &:last-child {
      margin-bottom: 0;
    }
  }
  
  .preview-image {
    width: 100%;
    min-height: 180px;
    max-height: 300px;
    display: flex;
    align-items: center;
    justify-content: center;
    background-color: var(--md-sys-color-surface-variant);
    
    img {
      width: 100%;
      height: 100%;
      object-fit: contain;
    }
  }
  
  .preview-content {
    padding: 16px;
    padding-inline-end: 48px;
  }

  .preview-actions {
    position: absolute;
    inset-block-start: 8px;
    inset-inline-end: 8px;
  }

  @media (hover: hover) {
    .link-preview-item:not(:hover) .preview-actions:not(:focus-within):not(.is-active) {
      opacity: 0;
      pointer-events: none;
    }
  }
  
  .preview-domain {
    font-size: 10px;
    font-weight: 500;
    color: var(--md-sys-color-primary);
    background-color: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
    padding: 4px 8px;
    border-radius: 4px;
    display: inline-block;
    margin-bottom: 8px;
    letter-spacing: 0.5px;
  }
  
  .preview-title {
    margin: 0 0 4px 0;
    color: var(--md-sys-color-on-surface);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  
  .preview-description {
    margin: 0 0 8px 0;
    font-size: 0.875rem;
    color: var(--md-sys-color-on-surface-variant);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  
  .preview-footer {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  
  .preview-small-icon {
    width: 18px;
    height: 18px;
    border-radius: 3px;
    object-fit: cover;
    flex-shrink: 0;
  }
  
  .preview-site {
    font-size: 0.875rem;
    color: color-mix(in srgb, var(--md-sys-color-on-surface-variant) 80%, transparent);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex: 1;
  }
}

@media (max-width: 768px) {
  .link-previews .link-preview-item {
    max-width: 100%;
    
    .preview-image {
      min-height: 100px;
      max-height: 160px;
    }
    
    .preview-content {
      padding: 12px;
    }
    
    .preview-title {
      font-size: 15px;
    }
    
    .preview-description {
      font-size: 13px;
      -webkit-line-clamp: 1;
    }
  }
}
</style>
