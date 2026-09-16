<template>
  <footer class="status-bar">
    <span v-if="kind === 'json' && jsonInvalid" class="verdict bad">
      <i-material-symbols:error-outline-rounded />
      {{ $t('json_invalid') }}
    </span>
    <span v-else-if="kind === 'json'" class="verdict ok">
      <i-material-symbols:check-circle-rounded />
      {{ $t('json_valid') }}
    </span>
    <span v-else-if="kind === 'xml'" class="verdict ok">
      <i-material-symbols:check-circle-rounded />
      {{ $t('xml_valid') }}
    </span>
    <span v-else-if="kind === 'doc'" class="verdict muted">
      {{ $t('doc_preview_hint') }}
    </span>
    <span v-else class="verdict ok">
      <i-material-symbols:check-circle-rounded />
      {{ $t('viewer_ready') }}
    </span>

    <button v-if="fixable" class="fix-chip" @click="$emit('fix')">
      <i-lucide-wrench />
      {{ $t('auto_fix') }}
    </button>
    <span v-if="jsonInvalid && errorPos" class="err-pos">
      {{ $t('error_line_col', { line: errorPos.line, col: errorPos.col }) }}
    </span>

    <span class="grow" />
    <span v-if="kind !== 'doc'" class="stat mono">{{ $t('n_lines', { n: lines }) }}</span>
    <span v-if="kind !== 'doc'" class="sep">·</span>
    <span class="stat mono">{{ sizeText }}</span>
    <span class="sep">·</span>
    <span class="stat">UTF-8</span>
  </footer>
</template>

<script setup lang="ts">
import type { FileKind } from './file-kind'

defineProps<{
  kind: FileKind
  jsonInvalid: boolean
  errorPos: { line: number; col: number } | null
  fixable: boolean
  lines: number
  sizeText: string
}>()

defineEmits<{
  fix: []
}>()
</script>

<style lang="scss" scoped>
.status-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 28px;
  padding: 0 16px;
  border-top: 1px solid var(--md-sys-color-outline-variant);
  background: var(--md-sys-color-surface);
  font-size: 0.75rem;
  color: var(--md-sys-color-on-surface-variant);
  flex-shrink: 0;
}

.verdict {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-weight: 600;

  &.ok {
    color: var(--jv-string, #15803d);

    svg {
      color: var(--jv-string, #15803d);
    }
  }

  &.bad {
    color: var(--md-sys-color-error);

    svg {
      color: var(--md-sys-color-error);
    }
  }

  &.muted {
    color: var(--md-sys-color-on-surface-variant);
    font-weight: 400;
  }

  svg {
    font-size: 14px;
  }
}

.fix-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: none;
  height: 22px;
  padding: 0 10px;
  border-radius: 999px;
  background: var(--jv-ts-bg, #fef3c7);
  color: var(--jv-ts-fg, #b45309);
  font-size: 0.6875rem;
  font-weight: 600;
  cursor: pointer;

  &:hover {
    filter: brightness(0.97);
  }

  svg {
    width: 12px;
    height: 12px;
  }

  :root.dark & {
    --jv-ts-fg: #fcd34d;
    --jv-ts-bg: rgba(146, 64, 14, 0.32);
  }
}

.err-pos {
  color: var(--md-sys-color-error);
  text-decoration: underline;
  cursor: pointer;
}

.grow {
  flex: 1;
}

.stat {
  white-space: nowrap;
}

.sep {
  color: var(--md-sys-color-outline-variant);
}

.mono {
  font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, monospace;
}
</style>
