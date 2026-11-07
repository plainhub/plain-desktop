<template>
  <!-- Header variant: single input, filter dropdown is inside TokenSearchField -->
  <div class="header-search">
    <TokenSearchField
      ref="inputRef"
      :key="String(globalSearch)"
      class="header-search-field"
      :text="globalSearch ? globalText : text"
      :tokens="globalSearch ? [] : uiTokens"
      enter-submits
      :placeholder="resolvedPlaceholder"
      :key-options="globalSearch || globalOnly ? [] : keyOptions"
      :value-options="globalSearch || globalOnly ? {} : valueOptions"
      @update:text="globalSearch ? globalText = $event : onFreeTextChange($event)"
      @update:tokens="!globalSearch && onUiTokensChange($event)"
      @enter="submit"
      @history:select="applyHistoryQ"
      @history:delete="deleteHistoryItem"
      @history:clear="clearHistoryForPage"
    >
      <template v-if="globalSearch || (enableGlobalSearch && (globalOnly || !hasPageFilters))" #start>
        <slot name="start" />
      </template>
    </TokenSearchField>
  </div>
</template>

<script setup lang="ts">
import type { IBucket, IFeed, IFilter, IFileFilter, ITag, IType } from '@/lib/interfaces'
import TokenSearchField from '@/components/TokenSearchField.vue'
import { useHeaderSearch } from '@/hooks/header-search'
import { nextTick, ref, watch } from 'vue'

type Kind = 'global' | 'media' | 'files'

const props = withDefaults(
  defineProps<{
    kind?: Kind
    globalSearch?: boolean
    globalKeyword?: string
    globalOnly?: boolean
    enableGlobalSearch?: boolean
    placeholder?: string
    enableSlashFocus?: boolean
    targetPath?: string
    syncRouteQ?: boolean
    filter?: IFilter
    getUrl?: (q: string) => string
    tags?: ITag[]
    buckets?: IBucket[]
    types?: IType[]
    showTrash?: boolean
    fileFilter?: IFileFilter
    getFileUrl?: (q: string) => string
    navigateToDir?: (dir: string) => void
  }>(),
  {
    kind: 'global',
    globalKeyword: '',
    placeholder: '',
    enableSlashFocus: true,
    targetPath: '',
    syncRouteQ: true,
    filter: undefined,
    getUrl: undefined,
    tags: () => [],
    buckets: () => [],
    types: () => [],
    showTrash: false,
    fileFilter: undefined,
    getFileUrl: undefined,
    navigateToDir: () => {},
  },
)
const globalText = ref('')
const emit = defineEmits<{ 'global-search': [text: string] }>()

const {
  inputRef, text, uiTokens, resolvedPlaceholder, keyOptions, valueOptions, hasPageFilters,
  onFreeTextChange, onUiTokensChange, submitFromHeader, applyHistoryQ, deleteHistoryItem, clearHistoryForPage,
} = useHeaderSearch(props)

function submit() {
  if (props.globalSearch) {
    emit('global-search', globalText.value)
  } else if (props.enableGlobalSearch && (props.globalOnly || (!hasPageFilters.value && text.value.trim()))) {
    globalText.value = text.value
    emit('global-search', globalText.value)
  } else if (!props.globalOnly) {
    submitFromHeader()
  }
}

watch(() => props.globalKeyword, (keyword) => { globalText.value = keyword }, { immediate: true })
watch(() => props.globalSearch, (active) => {
  if (active) void nextTick(() => inputRef.value?.focus())
}, { immediate: true })

defineExpose({ focus: () => inputRef.value?.focus() })
</script>

<style lang="scss" scoped>
.header-search {
  min-width: min(520px, 46vw);
}

.header-search :deep(.header-search-field) {
  width: 100%;
}
</style>
