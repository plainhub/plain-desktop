<template>
  <FileItemLayout
    :item="item" :name="item.displayName" :index="index" :is-phone="isPhone"
    :selected="selected" :checked="selected"
    @click="emit('toggle')" @checkbox-click="emit('toggle')"
  >
    <template #image>
      <div class="image">
        <FileThumb
          :is-dir="isDir" :thumb-url="thumbUrl" :extension="getFileExtension(item.displayName)"
          :thumb-error="thumbError" :ext-error="extError"
          :on-thumb-error="() => thumbError = true" :on-ext-error="() => extError = true"
        />
      </div>
    </template>
    <template #title>{{ item.displayName }}</template>
    <template #subtitle><span class="original-path" :title="item.originalPath">{{ item.originalPath }}</span></template>
    <template #actions>
      <div class="actions">
        <v-icon-button class="sm" :aria-label="$t('restore')" v-tooltip="$t('restore')" :disabled="busy" @click.stop="emit('restore')">
          <i-material-symbols:restore-from-trash-outline-rounded />
        </v-icon-button>
        <v-icon-button :id="anchor" class="sm" :aria-label="$t('delete')" v-tooltip="$t('delete')" :disabled="busy" @click.stop="emit('delete', anchor)">
          <i-material-symbols:delete-forever-outline-rounded />
        </v-icon-button>
      </div>
    </template>
  </FileItemLayout>
</template>

<script setup lang="ts">
import { computed, ref, useId } from 'vue'
import type { GqlResultType } from '@/lib/api/graphql/operations'
import { getFileExtension } from '@/lib/api/file'
import { fileThumbUrl } from '@/lib/file'
import { useTempStore } from '@/stores/temp'
import FileItemLayout from './FileItemLayout.vue'

const props = defineProps<{
  item: GqlResultType<'trashedFilesGQL'>['trashedFiles'][number]
  index: number
  selected: boolean
  isPhone: boolean
  busy: boolean
}>()
const emit = defineEmits<{ toggle: []; restore: []; delete: [anchor: string] }>()
const anchor = useId()
const temp = useTempStore()
const isDir = computed(() => props.item.type === 'DIR')
const thumbUrl = computed(() => fileThumbUrl(temp.urlTokenKey, { path: props.item.trashedPath, name: props.item.displayName, isDir: isDir.value }))
const thumbError = ref(false)
const extError = ref(false)
</script>

<style scoped>
.original-path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.file-item :deep(.subtitle),
.list-item-phone :deep(.phone-content) {
  min-width: 0;
}
</style>
