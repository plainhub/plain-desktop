<template>
  <div class="link-previews">
    <div v-for="preview in linkPreviews" :key="preview.url" class="link-preview-item" @click="openLink(preview.url)">
      <div v-if="shouldShowLargeImage(preview)" class="preview-image">
        <img 
          :src="getImageUrl(preview)" 
          :alt="preview.title"
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
            v-if="shouldShowSmallIcon(preview)"
            :src="getImageUrl(preview)"
            class="preview-small-icon"
            @error="onImageError(preview)"
          />
          <span class="preview-site">{{ preview.siteName || preview.url }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { getFileId, getFileUrl } from '@/lib/api/file'
import { openUrl } from '@/lib/browser'
import { useTempStore } from '@/stores/temp'

const props = defineProps({
  data: { type: Object, required: true },
})

const tempStore = useTempStore()

const linkPreviews = computed(() => {
  const data = props.data
  const previews = data?._content?.value?.linkPreviews ?? []
  return previews.map((preview: any) => {
    // Link-preview image ids are client-derived: encrypt the bare
    // imageLocalPath with the urlToken (no JSON wrapping — /fs parity).
    const fileId = preview.imageLocalPath ? getFileId(tempStore.urlTokenKey, preview.imageLocalPath) : ''
    return {
      ...preview,
      _fileId: fileId
    }
  })
})

function getImageUrl(preview: any) {
  if (preview._fileId && preview._fileId.trim() !== '') {
    return getFileUrl(preview._fileId)
  }
  return preview.imageUrl
}

function shouldShowLargeImage(preview: any) {
  if (!preview.imageUrl || preview.hasError) return false
  return preview.imageWidth >= 200 && preview.imageHeight >= 200
}

function shouldShowSmallIcon(preview: any) {
  if (!preview.imageUrl || preview.hasError) return false
  return preview.imageWidth < 200 || preview.imageHeight < 200
}

function onImageError(preview: any) {
  preview.hasError = true
}

function openLink(url: string) {
  openUrl(url)
}
</script>

<style lang="scss" scoped>
.link-previews {
  margin-top: 6px;
  max-width: 600px;
  
  .link-preview-item {
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