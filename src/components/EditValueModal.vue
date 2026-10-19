<template>
  <v-modal @close="cancel">
    <template #headline>
      {{ title }}
    </template>
    <template #content>
      <v-text-field ref="inputRef" v-model="name" autocapitalize="off" autocorrect="off" :spellcheck="false" :placeholder="placeholder" :error="!!errors.name" :error-text="errors.name ? $t(errors.name) : ''" @keyup.enter="doAction" />
    </template>
    <template #actions>
      <v-outlined-button value="cancel" @click="cancel">{{ $t('cancel') }}</v-outlined-button>
      <v-filled-button value="save" :loading="loading" @click="doAction">
        {{ $t('save') }}
      </v-filled-button>
    </template>
  </v-modal>
</template>
<script setup lang="ts">
import { onMounted, ref, reactive, type PropType, nextTick } from 'vue'
import { popModal } from './modal'

const inputRef = ref<HTMLInputElement>()

const props = defineProps({
  getVariables: {
    type: Function as PropType<(value: string) => Record<string, any>>,
    required: true,
  },
  title: { type: String, required: true },
  placeholder: { type: String, default: '' },
  value: { type: String, default: '' },
  mutation: { type: Function, required: true },
  done: {
    type: Function as PropType<(value: string) => void>,
    default: () => {},
  },
})

const name = ref(props.value ?? '')
const errors = reactive({ name: '' })

const { mutate, loading, onDone } = props.mutation()

function cancel() {
  popModal()
}

onMounted(async () => {
  await nextTick()
  requestAnimationFrame(() => {
    setTimeout(() => {
      try {
        if (document.activeElement && document.activeElement !== document.body) {
          (document.activeElement as HTMLElement).blur()
        }
        inputRef.value?.focus()
      } catch (error) {
        console.debug('Focus blocked:', error)
      }
    }, 100)
  })
})

function doAction() {
  if (!name.value?.trim()) { errors.name = 'valid.required'; return }
  errors.name = ''
  mutate(props.getVariables(name.value))
}

onDone(() => {
  props.done?.call(undefined, name.value!)
  popModal()
})
</script>
<style scoped lang="scss">
:deep(.form-control) {
  width: 100%;
}
</style>
