<template>
  <div class="actions">
    <template v-if="item.isUninstalling">
      <v-circular-progress v-tooltip="$t('uninstalling')" indeterminate class="sm" />
      &nbsp;<v-outlined-button @click.stop="cancelUninstall">{{ $t('cancel') }}</v-outlined-button>
    </template>
    <template v-else>
      <v-icon-button v-tooltip="$t('uninstall')" class="sm" @click.stop="uninstall">
        <i-material-symbols:delete-forever-outline-rounded />
      </v-icon-button>
      <v-icon-button v-tooltip="$t('download')" class="sm" @click.stop="download">
        <i-material-symbols:download-rounded />
      </v-icon-button>
    </template>
  </div>
</template>

<script setup lang="ts">
import type { IPackageItem } from '@/lib/interfaces'

interface Props {
  item: IPackageItem
}

defineProps<Props>()

const emit = defineEmits<{
  uninstall: []
  download: []
  cancelUninstall: []
}>()

function uninstall() {
  emit('uninstall')
}

function download() {
  emit('download')
}

function cancelUninstall() {
  emit('cancelUninstall')
}
</script>