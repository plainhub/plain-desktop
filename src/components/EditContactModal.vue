<template>
  <v-modal @close="popModal">
    <template #headline>
      {{ data ? $t('edit') : $t('create') }}
    </template>
    <template #content>
      <label class="form-label">
        {{ $t('name') }}
      </label>
      <div class="form-row">
        <v-text-field v-model="editItem.firstName" :label="$t('first_name')" />
        <v-text-field v-if="complexName" v-model="editItem.middleName" :label="$t('middle_name')" />
        <v-text-field v-model="editItem.lastName" :label="$t('last_name')" />
        <div v-if="!complexName" class="v-center">
          <button class="btn-icon" @click="complexName = true">
            
            <i-material-symbols:keyboard-double-arrow-down-rounded />
          </button>
        </div>
      </div>
      <div v-if="complexName" class="form-row">
        <v-text-field v-model="editItem.prefix" :label="$t('prefix')" />
        <v-text-field v-model="editItem.suffix" :label="$t('suffix')" />
        <div class="v-center">
          <button class="btn-icon" @click="complexName = false">
            
            <i-material-symbols:keyboard-double-arrow-up-rounded />
          </button>
        </div>
      </div>
      <label class="form-label">
        {{ $t('phone_number') }}
      </label>
      <div v-for="(item, index) in editItem.phoneNumbers" :key="index" class="form-row">
        <v-select 
          v-model="item.type" 
          class="flex-2" 
          :options="createTypeOptions(types.phoneNumberTypes, 'phone_number_type', item)"
          @change="onTypeChanged(item)"
        />
        <v-text-field v-model="item.value" :placeholder="$t('telephone')" class="flex-3" />
        <div class="v-center">
          <button v-if="editItem.phoneNumbers.length > 1" class="btn-icon" @click="deleteField(editItem.phoneNumbers, index)">
            
            <i-material-symbols:delete-forever-outline-rounded />
          </button>
        </div>
      </div>
      <label v-if="editItem.emails.length" class="form-label">
        {{ $t('email') }}
      </label>
      <div v-for="(item, index) in editItem.emails" :key="index" class="form-row">
        <v-select 
          v-model="item.type" 
          class="flex-2" 
          :options="createTypeOptions(types.emailTypes, 'email_type', item)"
          @change="onTypeChanged(item)"
        />
        <v-text-field v-model="item.value" :label="$t('email')" class="flex-3" />
        <div class="v-center">
          <button class="btn-icon" @click="deleteField(editItem.emails, index)">
            
            <i-material-symbols:delete-forever-outline-rounded />
          </button>
        </div>
      </div>
      <label v-if="editItem.addresses.length" class="form-label">
        {{ $t('address') }}
      </label>
      <div v-for="(item, index) in editItem.addresses" :key="index" class="form-row">
        <v-select 
          v-model="item.type" 
          class="flex-2" 
          :options="createTypeOptions(types.addressTypes, 'address_type', item)"
          @change="onTypeChanged(item)"
        />
        <v-text-field v-model="item.value" :label="$t('address')" class="flex-3" />
        <div class="v-center">
          <button class="btn-icon" @click="deleteField(editItem.addresses, index)">
            
            <i-material-symbols:delete-forever-outline-rounded />
          </button>
        </div>
      </div>
      <label v-if="editItem.websites.length" class="form-label">
        {{ $t('website') }}
      </label>
      <div v-for="(item, index) in editItem.websites" :key="index" class="form-row">
        <v-select 
          v-model="item.type" 
          class="flex-2" 
          :options="createTypeOptions(types.websiteTypes, 'website_type', item)"
          @change="onTypeChanged(item)"
        />
        <v-text-field v-model="item.value" :placeholder="$t('website')" class="flex-3" />
        <div class="v-center">
          <button class="btn-icon" @click="deleteField(editItem.websites, index)">
            
            <i-material-symbols:delete-forever-outline-rounded />
          </button>
        </div>
      </div>
      <label v-if="editItem.ims.length" class="form-label">
        {{ $t('im') }}
      </label>
      <div v-for="(item, index) in editItem.ims" :key="index" class="form-row">
        <v-select 
          v-model="item.protocol" 
          class="flex-2" 
          :options="createTypeOptions(types.imProtocols, 'im_protocol', item)"
          @change="onTypeChanged(item)"
        />
        <v-text-field v-model="item.value" :placeholder="$t('im')" class="flex-3" />
        <div class="v-center">
          <button class="btn-icon" @click="deleteField(editItem.ims, index)">
            
            <i-material-symbols:delete-forever-outline-rounded />
          </button>
        </div>
      </div>
      <div class="form-row" style="display: block; position: relative">
        <v-dropdown v-model="addFieldMenuVisible" placement="auto">
          <template #trigger>
            <v-outlined-button>{{ $t('add_field') }}</v-outlined-button>
          </template>
          <div class="dropdown-item" @click="() => addField(editItem.phoneNumbers, () => ({ type: 'MOBILE', value: '', label: '' }))">
            {{ $t('phone_number') }}
          </div>
          <div class="dropdown-item" @click="() => addField(editItem.emails, () => ({ type: 'HOME', value: '', label: '' }))">
            {{ $t('email') }}
          </div>
          <div class="dropdown-item" @click="() => addField(editItem.addresses, () => ({ type: 'HOME', value: '', label: '' }))">
            {{ $t('address') }}
          </div>
          <div class="dropdown-item" @click="() => addField(editItem.websites, () => ({ type: 'HOMEPAGE', value: '', label: '' }))">
            {{ $t('website') }}
          </div>
          <div class="dropdown-item" @click="() => addField(editItem.ims, () => ({ protocol: 'CUSTOM', value: '', customProtocol: '' }))">
            {{ $t('im') }}
          </div>
        </v-dropdown>
      </div>
      <div class="form-row">
        <v-text-field v-model="editItem.notes" type="textarea" :label="$t('notes')" :rows="3" />
      </div>
    </template>
    <template #actions>
      <v-outlined-button value="cancel" @click="popModal">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button value="save" :loading="createLoading || editLoading" @click="doAction">
        {{ $t('save') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>
<script setup lang="ts">
import type { PropType } from 'vue'
import type { IContact } from '@/lib/interfaces'
import { popModal } from './modal'
import { useEditContact } from '@/hooks/edit-contact'

const props = defineProps({
  data: { type: Object as PropType<IContact>, default: () => ({}) },
  sources: { type: Array as PropType<any[]>, default: () => [] },
  done: { type: Function as PropType<() => void>, default: () => {} },
})

const {
  editItem, complexName, addFieldMenuVisible, createLoading, editLoading, types,
  onTypeChanged, createTypeOptions, addField, deleteField, doAction,
} = useEditContact(props.data, props.sources, props.done)
</script>
<style lang="scss" scoped></style>
