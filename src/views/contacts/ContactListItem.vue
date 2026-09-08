<template>
  <section
    v-if="!isPhone"
    class="contact-item selectable-card"
    :class="{ selected: selectedIds.includes(item.id), selecting: shiftEffectingIds.includes(item.id) }"
    @click.stop="handleItemClick($event, item, index, () => {})"
    @mouseenter.stop="handleMouseOver($event, index)"
  >
    <div class="start">
      <v-checkbox v-if="shiftEffectingIds.includes(item.id)" class="checkbox" touch-target="wrapper" :checked="shouldSelect" @click.stop="toggleSelect($event, item, index)" />
      <v-checkbox v-else class="checkbox" touch-target="wrapper" :checked="selectedIds.includes(item.id)" @click.stop="toggleSelect($event, item, index)" />
      <span class="number"><field-id :id="index + 1" :raw="item" /></span>
    </div>
    <img v-if="item.thumbnailId" class="image" :src="getFileUrl(item.thumbnailId)" width="50" loading="lazy" decoding="async" />
    <i-material-symbols:contact-page-outline-rounded v-else class="image" />
    <div class="title">{{ fullName(item) }}</div>
    <div class="subtitle">
      <span v-if="item.notes">{{ item.notes }}</span>
      <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
    </div>
    <div class="info">
      <ul class="list-unstyled">
        <li v-for="(it, phoneIndex) in item.phoneNumbers" :key="phoneIndex" class="phone-number">
          {{ it.type > 0 ? $t(`contact.phone_number_type.${it.type}`) : it.label }}
          {{ it.normalizedNumber || it.value }}
          <v-icon-button v-tooltip="$t('send_sms')" @click.stop="sendSms(item.id, it.normalizedNumber || it.value, phoneIndex)">
            <i-material-symbols:sms-outline-rounded />
          </v-icon-button>
          <v-icon-button v-tooltip="$t('make_a_phone_call')" :loading="callLoading && callId === item.id && callIndex === phoneIndex" @click.stop="call(item.id, it.normalizedNumber || it.value, phoneIndex)">
            <i-material-symbols:call-outline-rounded />
          </v-icon-button>
        </li>
        <li v-for="it in item.emails" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.email_type.${it.type}`) : it.label }} {{ it.value }}</li>
        <li v-for="it in item.addresses" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.address_type.${it.type}`) : it.label }} {{ it.value }}</li>
        <li v-for="it in item.websites" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.website_type.${it.type}`) : it.label }} {{ it.value }}</li>
        <li v-for="it in item.ims" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.im_type.${it.type}`) : it.label }} {{ it.value }}</li>
        <li v-for="it in item.events" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.event_type.${it.type}`) : it.label }} {{ it.value }}</li>
      </ul>
    </div>
    <ContactActionButtons
      :item="item"
      :tags="tags"
      :data-type="dataType"
      @delete-item="deleteItem"
      @edit="edit"
    />
    <div class="time">
      <span v-tooltip="formatDateTime(item.updatedAt)">
        {{ formatTimeAgo(item.updatedAt) }}
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
    <template #image>
      <img v-if="item.thumbnailId" class="image" :src="getFileUrl(item.thumbnailId)" width="50" loading="lazy" decoding="async" />
      <i-material-symbols:contact-page-outline-rounded v-else class="image" />
    </template>
    
    <template #title>{{ fullName(item) }}</template>
    
    <template #subtitle>
      <div class="subtitle">
        <span v-if="item.notes">{{ item.notes }}</span>
        <item-tags :tags="item.tags" :type="dataType" :only-links="true" />
      </div>
      <div class="info">
        <ul class="list-unstyled">
          <li v-for="(it, phoneIndex) in item.phoneNumbers" :key="phoneIndex" class="phone-number">
            {{ it.type > 0 ? $t(`contact.phone_number_type.${it.type}`) : it.label }}
            {{ it.normalizedNumber || it.value }}
            <v-icon-button v-tooltip="$t('send_sms')" @click.stop="sendSms(item.id, it.normalizedNumber || it.value, phoneIndex)">
              <i-material-symbols:sms-outline-rounded />
            </v-icon-button>
            <v-icon-button v-tooltip="$t('make_a_phone_call')" :loading="callLoading && callId === item.id && callIndex === phoneIndex" @click.stop="call(item.id, it.normalizedNumber || it.value, phoneIndex)">
              <i-material-symbols:call-outline-rounded />
            </v-icon-button>
          </li>
          <li v-for="it in item.emails" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.email_type.${it.type}`) : it.label }} {{ it.value }}</li>
          <li v-for="it in item.addresses" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.address_type.${it.type}`) : it.label }} {{ it.value }}</li>
          <li v-for="it in item.websites" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.website_type.${it.type}`) : it.label }} {{ it.value }}</li>
          <li v-for="it in item.ims" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.im_type.${it.type}`) : it.label }} {{ it.value }}</li>
          <li v-for="it in item.events" :key="it.type + it.value">{{ it.type > 0 ? $t(`contact.event_type.${it.type}`) : it.label }} {{ it.value }}</li>
        </ul>
      </div>
      <div class="time">
        <span v-tooltip="formatDateTime(item.updatedAt)">
          {{ formatTimeAgo(item.updatedAt) }}
        </span>
      </div>
    </template>
    
    <template #actions>
      <ContactActionButtons
        :item="item"
        :tags="tags"
        :data-type="dataType"
        @delete-item="deleteItem"
        @edit="edit"
      />
    </template>
  </ListItemPhone>
</template>

<script setup lang="ts">
import type { IContact, ITag } from '@/lib/interfaces'
import { DataType } from '@/lib/data'
import { formatDateTime, formatTimeAgo } from '@/lib/format'
import { getFileUrl } from '@/lib/api/file'
import { getContactFullName } from '@/lib/contact/format'
import ContactActionButtons from './ContactActionButtons.vue'

interface Props {
  item: IContact
  index: number
  selectedIds: string[]
  shiftEffectingIds: string[]
  shouldSelect: boolean
  isPhone: boolean
  dataType: DataType
  tags: ITag[]
  callLoading?: boolean
  callId?: string
  callIndex?: number
  // Functions passed from parent
  handleItemClick: (event: MouseEvent, item: IContact, index: number, callback: () => void) => void
  handleMouseOver: (event: MouseEvent, index: number) => void
  toggleSelect: (event: MouseEvent, item: IContact, index: number) => void
}

defineProps<Props>()

const emit = defineEmits<{
  deleteItem: [item: IContact]
  edit: [item: IContact]
  sendSms: [id: string, number: string, index: number]
  call: [id: string, number: string, index: number]
}>()

function deleteItem(item: IContact) {
  emit('deleteItem', item)
}

function edit(item: IContact) {
  emit('edit', item)
}

function call(id: string, number: string, index: number) {
  emit('call', id, number, index)
}

function sendSms(id: string, number: string, index: number) {
  emit('sendSms', id, number, index)
}

function fullName(item: IContact) {
  return getContactFullName(item)
}
</script>

<style scoped lang="scss">
.list-unstyled {
  list-style: none;
  margin: 0;
  padding: 0;
}

.main-list .list-item-phone {
  gap: 8px;
  
  .image {
    width: 50px;
    height: 50px;
    object-fit: cover;
    border-radius: 8px;
  }
}
</style> 