<template>
  <button
    type="button"
    class="v-copy-button"
    :class="{ copied }"
    :aria-label="$t('copy')"
    @click.stop="copy"
  >
    <i-material-symbols:check-rounded v-if="copied" class="copied-ico" />
    <slot v-else name="icon">
      <i-lucide-copy />
    </slot>
  </button>
</template>

<script setup lang="ts">
import { onUnmounted, ref } from 'vue'
import { copyTextToClipboard } from '@/lib/clipboard'

const props = defineProps<{ text: string }>()

const copied = ref(false)
let timer: ReturnType<typeof setTimeout> | undefined

async function copy() {
  const ok = await copyTextToClipboard(props.text)
  if (!ok) return
  copied.value = true
  clearTimeout(timer)
  timer = setTimeout(() => (copied.value = false), 1500)
}

onUnmounted(() => clearTimeout(timer))
</script>

<style scoped>
.v-copy-button {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border: none;
  padding: 0;
  background: transparent;
  color: inherit;
  cursor: pointer;
}

.v-copy-button.v-copy-button.copied {
  color: var(--md-sys-color-primary);
}

.v-copy-button .copied-ico {
  animation: v-copy-pop 0.3s ease;
}

@keyframes v-copy-pop {
  0% {
    transform: scale(0.4);
    opacity: 0;
  }
  60% {
    transform: scale(1.25);
  }
  100% {
    transform: scale(1);
    opacity: 1;
  }
}
</style>
