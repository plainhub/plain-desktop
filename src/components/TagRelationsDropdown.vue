<template>
  <v-dropdown v-model="visible" strategy="below" align="end" @click.stop>
    <template #trigger>
      <v-icon-button v-tooltip="$t('add_to_tags')" class="sm">
        <i-material-symbols:label-outline-rounded />
      </v-icon-button>
    </template>
    <div class="tag-picker" @click.stop>
      <div class="tag-picker-search">
        <input
          v-model="query"
          type="text"
          :placeholder="$t('search_or_create')"
          class="search-input"
          @click.stop
        />
      </div>
      <div class="tag-picker-list">
        <div
          v-for="tag in filteredTags"
          :key="tag.id"
          class="tag-picker-item"
          :class="{ 'is-loading': loading }"
          @click="toggleTag(tag)"
        >
          <v-checkbox class="tag-checkbox" :checked="selectedTagIds.includes(tag.id)" />
          <span class="tag-name">{{ tag.name }}</span>
        </div>
        <div v-if="canCreate" class="tag-picker-item create-item" @click="createAndAdd">
          <i-material-symbols:add class="add-icon" />
          {{ $t('create') }}: "{{ query }}"
        </div>
        <div v-if="filteredTags.length === 0 && !canCreate" class="tag-picker-empty">{{ $t('no_data') }}</div>
      </div>
    </div>
  </v-dropdown>
</template>

<script setup lang="ts">
import { computed, ref, type PropType } from 'vue'
import { initMutation, updateTagRelationsGQL, createTagGQL } from '@/lib/api/mutation'
import type { ITag, ITagRelationItem } from '@/lib/interfaces'
import emitter from '@/plugins/eventbus'

const props = defineProps({
  type: { type: String, required: true },
  tags: { type: Array as PropType<ITag[]>, default: () => [] },
  item: { type: Object as PropType<ITagRelationItem>, required: true },
  selected: { type: Array as PropType<ITag[]>, default: () => [] },
})

const visible = ref(false)
const query = ref('')

const selectedTagIds = computed(() => props.selected.map((it) => it.id))

const filteredTags = computed(() => {
  const q = query.value.trim().toLowerCase()
  return q ? props.tags.filter((t) => t.name.toLowerCase().includes(q)) : props.tags
})

const canCreate = computed(() => {
  const q = query.value.trim()
  return q.length > 0 && !props.tags.some((t) => t.name.toLowerCase() === q.toLowerCase())
})

const { mutate, loading, onDone } = initMutation({ document: updateTagRelationsGQL })

onDone(() => {
  emitter.emit('item_tags_updated', { item: props.item, type: props.type })
  emitter.emit('refetch_tags', props.type)
})

function toggleTag(tag: ITag) {
  if (loading.value) return
  const selected = selectedTagIds.value.includes(tag.id)
  mutate({
    type: props.type,
    addTagIds: selected ? [] : [tag.id],
    keys: [props.item.key],
    removeTagIds: selected ? [tag.id] : [],
  })
}

function createAndAdd() {
  const name = query.value.trim()
  if (!name) return
  const m = initMutation({ document: createTagGQL })
  m.onDone((result) => {
    const newTag: ITag = result.data?.createTag
    if (!newTag) return
    emitter.emit('refetch_tags', props.type)
    mutate({
      type: props.type,
      addTagIds: [newTag.id],
      keys: [props.item.key],
      removeTagIds: [],
    })
    query.value = ''
  })
  m.mutate({ type: props.type, name })
}
</script>

<style scoped lang="scss">
.tag-picker {
  min-width: 220px;
  max-width: 280px;
}

.tag-picker-search {
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

.tag-picker-list {
  max-height: 240px;
  overflow-y: auto;
  padding: 4px 0;
}

.tag-picker-item {
  display: flex;
  align-items: center;
  padding: 9px 14px;
  font-size: 14px;
  cursor: pointer;
  border-radius: 0;

  &:hover {
    background: var(--md-sys-color-surface-container-high);
  }

  &.is-selected {
    background: color-mix(in srgb, var(--md-sys-color-primary) 10%, transparent);
    color: var(--md-sys-color-primary);
  }

  &.is-loading {
    opacity: 0.5;
    pointer-events: none;
  }

  .tag-checkbox {
    flex-shrink: 0;
    pointer-events: none;
  }

  .tag-name {
    flex: 1;
  }
}

.create-item {
  color: var(--md-sys-color-primary);
  border-top: 1px solid var(--md-sys-color-outline-variant);
  margin-top: 4px;
  gap: 4px;

  .add-icon {
    font-size: 16px;
  }
}

.tag-picker-empty {
  padding: 12px;
  font-size: 14px;
  color: var(--md-sys-color-on-surface-variant);
  text-align: center;
}
</style>

