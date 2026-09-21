<!-- eslint-disable vue/no-v-html -->
<template>
  <section
    v-if="!isPhone"
    class="sms-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => handleView(index, item))"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    <div class="title">
      {{ getDisplayName(item.address) }}
      <span v-if="item.type === 'FAILED'" v-tooltip="$t('message_type.FAILED')" class="failed-icon">&#x26A0;</span>
    </div>
    <div class="subtitle" v-html="addLinksToURLs(item.body)"></div>
    <MessageActionButtons
      :item="item"
      :tags="tags"
      :data-type="dataType"
      :call-loading="callLoading"
      :call-id="callId"
      @send-sms="sendSms"
      @call="call"
      @archive="archive"
    />
    <div class="info">
      <span :class="{ 'text-red': item.type === 'FAILED' }">{{ $t(`message_type.${item.type}`) }}</span>
      <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
    </div>
    <div class="time">
      <span v-tooltip="formatDateTime(item.sentAt)">
        {{ formatTimeAgo(item.sentAt) }}
      </span>
    </div>
  </section>

  <!-- Phone Layout -->
  <ListItemPhone
    v-else
    :is-selected="selectedIds.includes(item.id)"
    :is-selecting="shiftEffectingIds.includes(item.id)"
    :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
    @click="handleItemClick($event, item, index, () => handleView(index, item))"
    @mouseenter.stop="handleMouseOver($event, index)"
    @checkbox-click="(event: MouseEvent) => toggleSelect(event, item, index)"
  >
    <template #title>
      {{ getDisplayName(item.address) }}
      <span v-if="item.type === 'FAILED'" v-tooltip="$t('message_type.FAILED')" class="failed-icon">&#x26A0;</span>
    </template>
    
    <template #subtitle>
      <div class="subtitle" v-html="addLinksToURLs(item.body)"></div>
      <div class="info">
        <span :class="{ 'text-red': item.type === 'FAILED' }">{{ $t(`message_type.${item.type}`) }}</span>
        <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
      </div>
      <div class="time">
        <span v-tooltip="formatDateTime(item.sentAt)">
          {{ formatTimeAgo(item.sentAt) }}
        </span>
      </div>
    </template>
    
    <template #actions>
      <MessageActionButtons
        :item="item"
        :tags="tags"
        :data-type="dataType"
        :call-loading="callLoading"
        :call-id="callId"
        @send-sms="sendSms"
        @call="call"
        @archive="archive"
      />
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import type { ISms, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { formatDateTime, formatTimeAgo } from '@/lib/format'
import { addLinksToURLs } from '@/lib/strutil'
import MessageActionButtons from './MessageActionButtons.vue'
import { useContactName } from '@/hooks/contacts'

const { getDisplayName } = useContactName()

interface Props {
  item: ISms
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  isPhone: boolean
  dataType: DataType
  tags: ITag[]
  callLoading?: boolean
  callId?: string
  // Functions passed from parent
  handleItemClick: (event: MouseEvent, item: ISms, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: ISms, index: number) => void
  onView?: (index: number, item: ISms) => void
}

const props = defineProps<Props>()

const emit = defineEmits<{
  sendSms: [item: ISms]
  call: [item: ISms]
  archive: [item: ISms]
}>()

function call(item: ISms) {
  emit('call', item)
}

function sendSms(item: ISms) {
  emit('sendSms', item)
}

function archive(item: ISms) {
  emit('archive', item)
}

function handleView(index: number, item: ISms) {
  props.onView?.(index, item)
}
</script>

<style scoped lang="scss">
.main-list .list-item-phone {
  gap: 8px;
}

.failed-icon {
  color: var(--md-sys-color-error, #d32f2f);
  font-size: 14px;
  margin-inline-start: 4px;
}

.text-red {
  color: var(--md-sys-color-error, #d32f2f);
}
</style>