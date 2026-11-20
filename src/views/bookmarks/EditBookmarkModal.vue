<template>
  <v-modal @close="$emit('close')">
    <template #headline>
      {{ $t('edit_bookmark') }}
    </template>
    <template #content>
      <div class="form-row">
        <v-text-field v-model="localTitle" :label="$t('title')" :placeholder="$t('bookmark_title_placeholder')" />
      </div>
      <div class="form-row">
        <v-text-field v-model="localUrl" label="URL" type="url" />
      </div>
      <div v-if="groups.length" class="form-row">
        <v-select v-model="localGroupId" :label="$t('add_to_group')" :options="groupOptions" />
      </div>
    </template>
    <template #actions>
      <v-outlined-button @click="$emit('close')">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button :loading="loading" :disabled="!localTitle.trim() || !localUrl.trim()" @click="save">
        {{ $t('save') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>

<script setup lang="ts">
import { showActionError } from '@/hooks/show-action-error'
import { ref, computed } from 'vue'
import { useI18n } from 'vue-i18n'
import type { Bookmark, BookmarkGroup } from '@/stores/bookmarks'
import { initMutation } from '@/lib/api/mutation'
import { updateBookmarkGQL } from '@/lib/api/mutation'

const props = defineProps<{
  bookmark: Bookmark
  groups: BookmarkGroup[]
}>()

const emit = defineEmits<{
  close: []
  saved: [bookmark: Bookmark]
}>()

const { t } = useI18n()
const localTitle = ref(props.bookmark.title)
const localUrl = ref(props.bookmark.url)
const localGroupId = ref(props.bookmark.groupId)

const groupOptions = computed(() => [
  { value: '', label: t('ungrouped') },
  ...props.groups.map((g) => ({ value: g.id, label: g.name })),
])

const { mutate, loading, onDone } = initMutation({
  document: updateBookmarkGQL,
}, showActionError)

onDone((result: any) => {
  if (result.data?.updateBookmark) {
    emit('saved', { ...result.data.updateBookmark })
  }
})

function save() {
  mutate({
    id: props.bookmark.id,
    input: {
      title: localTitle.value.trim(),
      url: localUrl.value.trim(),
      groupId: localGroupId.value,
      pinned: props.bookmark.pinned,
      sortOrder: props.bookmark.sortOrder,
    },
  })
}
</script>
