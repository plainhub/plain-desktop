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
      <ContactFieldRow
        v-for="(item, index) in editItem.phoneNumbers"
        :key="index"
        v-model="item.value"
        :placeholder="$t('telephone')"
        :removable="editItem.phoneNumbers.length > 1"
        @remove="deleteField(editItem.phoneNumbers, index)"
      >
        <template #selector>
          <v-select
            v-model="item.type"
            :options="createTypeOptions(types.phoneNumberTypes, 'phone_number_type', item)"
            @change="onTypeChanged(item)"
          />
        </template>
      </ContactFieldRow>
      <label v-if="editItem.emails.length" class="form-label">
        {{ $t('email') }}
      </label>
      <ContactFieldRow
        v-for="(item, index) in editItem.emails"
        :key="index"
        v-model="item.value"
        :label="$t('email')"
        removable
        @remove="deleteField(editItem.emails, index)"
      >
        <template #selector>
          <v-select
            v-model="item.type"
            :options="createTypeOptions(types.emailTypes, 'email_type', item)"
            @change="onTypeChanged(item)"
          />
        </template>
      </ContactFieldRow>
      <label v-if="editItem.addresses.length" class="form-label">
        {{ $t('address') }}
      </label>
      <ContactFieldRow
        v-for="(item, index) in editItem.addresses"
        :key="index"
        v-model="item.value"
        :label="$t('address')"
        removable
        @remove="deleteField(editItem.addresses, index)"
      >
        <template #selector>
          <v-select
            v-model="item.type"
            :options="createTypeOptions(types.addressTypes, 'address_type', item)"
            @change="onTypeChanged(item)"
          />
        </template>
      </ContactFieldRow>
      <label v-if="editItem.websites.length" class="form-label">
        {{ $t('website') }}
      </label>
      <ContactFieldRow
        v-for="(item, index) in editItem.websites"
        :key="index"
        v-model="item.value"
        :placeholder="$t('website')"
        removable
        @remove="deleteField(editItem.websites, index)"
      >
        <template #selector>
          <v-select
            v-model="item.type"
            :options="createTypeOptions(types.websiteTypes, 'website_type', item)"
            @change="onTypeChanged(item)"
          />
        </template>
      </ContactFieldRow>
      <label v-if="editItem.ims.length" class="form-label">
        {{ $t('im') }}
      </label>
      <ContactFieldRow
        v-for="(item, index) in editItem.ims"
        :key="index"
        v-model="item.value"
        :placeholder="$t('im')"
        removable
        @remove="deleteField(editItem.ims, index)"
      >
        <template #selector>
          <v-select
            v-model="item.protocol"
            :options="createTypeOptions(types.imProtocols, 'im_protocol', item)"
            @change="onTypeChanged(item)"
          />
        </template>
      </ContactFieldRow>
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
import ContactFieldRow from './ContactFieldRow.vue'
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
