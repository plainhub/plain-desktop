<template>
  <v-dropdown v-model="visible" @click.stop>
    <template #trigger>
      <v-icon-button v-tooltip="$t('add_to_tags')">
        <i-material-symbols:label-outline-rounded />
      </v-icon-button>
    </template>
    <div class="bulk-tag-picker" @click.stop>
      <div class="bulk-tag-picker-search">
        <input
          v-model="search"
          type="text"
          :placeholder="$t('search_or_create')"
          class="search-input"
          @click.stop
          @keydown.stop
        />
      </div>
      <div class="bulk-tag-picker-list">
        <div
          v-for="tag in filteredTags"
          :key="tag.id"
          class="bulk-tag-item"
          :class="{ 'is-loading': adding || removing || creating }"
          @click.stop="toggleTag(tag)"
        >
          <v-checkbox class="tag-checkbox" :checked="getTagState(tag) === 'all'" :indeterminate="getTagState(tag) === 'some'" />
          <span class="tag-name">{{ tag.name }}</span>
          <i-lucide:circle-minus v-if="getTagState(tag) === 'some'" v-tooltip="$t('clear_tags_from_selected')" class="clear-icon" @click.stop="clearTag(tag)" />
        </div>
        <div v-if="canCreate" class="bulk-tag-item create-item" @click.stop="createAndAdd">
          <i-material-symbols:add class="add-icon" />
          {{ $t('create') }}: "{{ search }}"
        </div>
        <div v-if="filteredTags.length === 0 && !canCreate" class="bulk-tag-empty">{{ $t('no_data') }}</div>
      </div>
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { computed, ref, type PropType } from 'vue'
import { addToTagsGQL, removeFromTagsGQL, createTagGQL, initMutation } from '@/lib/api/mutation'
import type { ITag } from '@/lib/interfaces'
import emitter from '@/plugins/eventbus'
import { useI18n } from 'vue-i18n'

const { t } = useI18n()

const props = defineProps({
  type: { type: String, required: true },
  tags: { type: Array as PropType<ITag[]>, default: () => [] },
  items: { type: Array as PropType<{ id: string; tags: { id: string }[] }[]>, default: () => [] },
  selectedIds: { type: Array as PropType<string[]>, default: () => [] },
  realAllChecked: { type: Boolean, default: false },
  q: { type: String, default: '' },
})

const visible = ref(false)
const search = ref('')

const effectiveQuery = computed(() =>
  props.realAllChecked ? props.q : `ids:${props.selectedIds.join(',')}`
)

const selectedItems = computed(() =>
  props.realAllChecked ? props.items : props.items.filter((i) => props.selectedIds.includes(i.id))
)

const filteredTags = computed(() => {
  const q = search.value.trim().toLowerCase()
  return q ? props.tags.filter((t) => t.name.toLowerCase().includes(q)) : props.tags
})

const canCreate = computed(() => {
  const q = search.value.trim()
  return q.length > 0 && !props.tags.some((t) => t.name.toLowerCase() === q.toLowerCase())
})

function getTagState(tag: ITag): 'all' | 'some' | 'none' {
  const count = selectedItems.value.filter((i) => i.tags.some((t) => t.id === tag.id)).length
  if (count === 0) return 'none'
  if (count === selectedItems.value.length) return 'all'
  return 'some'
}

const { mutate: addToTags, loading: adding, onDone: onAdded } = initMutation({ document: addToTagsGQL }, showActionError)
const { mutate: removeFromTags, loading: removing, onDone: onRemoved } = initMutation({ document: removeFromTagsGQL }, showActionError)
const { mutate: createTag, loading: creating, onDone: onCreated } = initMutation({ document: createTagGQL }, showActionError)

const onDone = () => {
  emitter.emit('items_tags_updated', { type: props.type })
  emitter.emit('refetch_tags', props.type)
}

onAdded(onDone)
onRemoved(onDone)

function toggleTag(tag: ITag) {
  if (adding.value || removing.value) return
  const state = getTagState(tag)
  const q = effectiveQuery.value
  if (state === 'all') {
    removeFromTags({ type: props.type, tagIds: [tag.id], query: q })
  } else {
    addToTags({ type: props.type, tagIds: [tag.id], query: q })
  }
}

function clearTag(tag: ITag) {
  if (adding.value || removing.value) return
  removeFromTags({ type: props.type, tagIds: [tag.id], query: effectiveQuery.value })
}

function createAndAdd() {
  const name = search.value.trim()
  if (!name || creating.value) return
  createTag({ type: props.type, name })
  onCreated((result: any) => {
    const newTag: ITag = result.data?.createTag
    if (!newTag) return
    emitter.emit('refetch_tags', props.type)
    addToTags({ type: props.type, tagIds: [newTag.id], query: effectiveQuery.value })
    search.value = ''
  })
}
</script>

<style scoped lang="scss">
.bulk-tag-picker {
  min-width: 260px;
  max-width: 320px;
}

.bulk-tag-picker-search {
  padding: 8px 12px;
  border-bottom: 1px solid var(--md-sys-color-outline-variant);

  .search-input {
    width: 100%;
    padding: 6px 10px;
    border: 1.5px solid var(--md-sys-color-outline-variant);
    border-radius: 6px;
    outline: none;
    background: transparent;
    font-size: 14px;
    color: var(--md-sys-color-on-surface);
    box-sizing: border-box;

    &:focus {
      border-color: var(--md-sys-color-primary);
    }

    &::placeholder {
      color: var(--md-sys-color-on-surface-variant);
    }
  }
}

.bulk-tag-picker-list {
  max-height: 280px;
  overflow-y: auto;
  padding: 4px 0;
}

.bulk-tag-item {
  display: flex;
  align-items: center;
  padding: 8px 14px;
  font-size: 14px;
  cursor: pointer;
  gap: 8px;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  &.is-loading {
    opacity: 0.5;
    pointer-events: none;
  }

  .tag-checkbox {
    flex-shrink: 0;
    pointer-events: none;
  }

  .clear-icon {
    flex-shrink: 0;
    font-size: 18px;
    color: var(--md-sys-color-error);
    cursor: pointer;
    opacity: 0.8;

    &:hover {
      opacity: 1;
    }
  }

  .tag-name {
    flex: 1;
    font-weight: 500;
  }
}

.bulk-tag-empty {
  padding: 16px 14px;
  font-size: 14px;
  color: var(--md-sys-color-on-surface-variant);
  text-align: center;
}

.create-item {
  color: var(--md-sys-color-primary);
  border-top: 1px solid var(--md-sys-color-outline-variant);
  margin-top: 4px;

  .add-icon {
    font-size: 16px;
  }
}
</style>
