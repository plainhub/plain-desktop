<template>
  <section
    v-if="!isPhone"
    class="call-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => {})"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>

    <div class="title">
      {{ item.name ? item.name + ' ' + item.number : item.number }}
    </div>
    <div class="subtitle">
      <span>{{ formatSeconds(item.duration) }}</span>
      <span>{{ $t('call_type.' + item.type) }}</span>
      <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
    </div>
    <CallActionButtons
      :item="item"
      :tags="tags"
      :data-type="dataType"
      :call-loading="callLoading"
      :call-id="callId"
      @delete-item="deleteItem"
      @call="call"
    />
    <div class="geo">
      {{ getGeoText(item.geo) }}
    </div>
    <div class="time">
      <span v-tooltip="formatDateTime(item.startedAt)">
        {{ formatTimeAgo(item.startedAt) }}
      </span>
    </div>
  </section>

  <!-- Phone Layout -->
  <ListItemPhone
    v-else
    :is-selected="selectedIds.includes(item.id)"
    :is-selecting="shiftEffectingIds.includes(item.id)"
    :checkbox-checked="shiftEffectingIds.includes(item.id) ? shouldSelect : selectedIds.includes(item.id)"
    @click="handleItemClick($event, item, index, () => {})"
    @mouseenter.stop="handleMouseOver($event, index)"
    @checkbox-click="(event: MouseEvent) => toggleSelect(event, item, index)"
  >
    <template #title>{{ item.name ? item.name + ' ' + item.number : item.number }}</template>
    
    <template #subtitle>
      <div class="subtitle">
        <span>{{ formatSeconds(item.duration) }}</span>
        <span>{{ $t('call_type.' + item.type) }}</span>
        <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
      </div>
      <div class="geo">
        {{ getGeoText(item.geo) }}
      </div>
      <div class="time">
        <span v-tooltip="formatDateTime(item.startedAt)">
          {{ formatTimeAgo(item.startedAt) }}
        </span>
      </div>
    </template>
    
    <template #actions>
      <CallActionButtons
        :item="item"
        :tags="tags"
        :data-type="dataType"
        :call-loading="callLoading"
        :call-id="callId"
        @delete-item="deleteItem"
        @call="call"
      />
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import type { ICall, ICallGeo, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { formatDateTime, formatSeconds, formatTimeAgo } from '@/lib/format'
import { useI18n } from 'vue-i18n'
import CallActionButtons from './CallActionButtons.vue'

interface Props {
  item: ICall
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
  handleItemClick: (event: MouseEvent, item: ICall, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: ICall, index: number) => void
}

defineProps<Props>()

const { t, locale } = useI18n()

const emit = defineEmits<{
  deleteItem: [item: ICall]
  call: [item: ICall]
}>()

function deleteItem(item: ICall) {
  emit('deleteItem', item)
}

function call(item: ICall) {
  emit('call', item)
}

function getGeoText(geo: ICallGeo | null | undefined) {
  if (!geo) {
    return ''
  }

  const texts = []
  if (geo.numberType && geo.numberType !== 'MOBILE') {
    texts.push(t('phone_number_type.' + geo.numberType))
  }
  if (geo.carrier) {
    texts.push(geo.carrier)
  }
  if (geo.description) {
    texts.push(geo.description)
  }
  if (geo.country) {
    const countryName = new Intl.DisplayNames([locale.value], { type: 'region' }).of(geo.country)
    if (countryName && countryName !== geo.country) {
      texts.push(countryName)
    }
  }

  return texts.join(', ')
}
</script>

<style scoped lang="scss">
.main-list .list-item-phone {
  gap: 8px;
}
</style> 