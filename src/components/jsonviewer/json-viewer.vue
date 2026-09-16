<template>
  <div ref="scrollRef" class="json-tree" @scroll="onScroll">
    <template v-if="hasData">
      <div class="json-tree-spacer" :style="{ height: `${totalHeight}px` }">
        <div class="json-tree-window" :style="{ top: `${startOffset}px` }">
          <div
            v-for="row in visibleRows"
            :key="row.rowKey"
            class="tree-row"
            :class="{ 'tree-row--open': row.kind === 'open' }"
            :style="{ height: `${ROW_HEIGHT}px` }"
            @click="row.kind === 'open' && togglePath(row.path, row.depth)"
          >
            <span class="jv-indent" :style="{ width: `${row.depth * 20}px` }" />
            <span v-if="row.kind === 'open'" class="jv-arrow" :class="{ open: !row.isCollapsed }">
              <svg viewBox="0 0 12 12"><path d="M4 2l4 4-4 4z" /></svg>
            </span>
            <span v-else class="jv-arrow" />

            <template v-if="row.kind === 'open'">
              <template v-if="xml">
                <span v-if="row.key" class="jv-key-xml" :class="{ 'jv-match': row.match }">&lt;{{ row.key }}&gt;</span>
                <span v-else-if="xmlRoot" class="jv-key-xml">&lt;{{ xmlRoot }}&gt;</span>
                <template v-if="row.isCollapsed">
                  <span class="jv-muted">… {{ row.childCount }}</span>
                  <span v-if="row.key || xmlRoot" class="jv-key-xml">&lt;/{{ row.key || xmlRoot }}&gt;</span>
                  <span v-if="row.trailingComma" class="jv-punct">,</span>
                  <span class="jv-hint">
                    {{ $t('json_n_items', { n: row.childCount }) }}
                  </span>
                </template>
              </template>
              <template v-else>
                <span v-if="row.showKey" class="jv-key" :class="{ 'jv-match': row.match }">"{{ row.key }}"</span>
                <span v-if="row.showKey" class="jv-punct">:&nbsp;</span>
                <span class="jv-punct">{{ row.isArray ? '[' : '{' }}</span>
                <template v-if="row.isCollapsed">
                  <span class="jv-muted">… {{ row.childCount }}</span>
                  <span class="jv-punct">{{ row.isArray ? ']' : '}' }}</span>
                  <span v-if="row.trailingComma" class="jv-punct">,</span>
                  <span class="jv-hint">
                    {{ row.isArray ? $t('json_n_items', { n: row.childCount }) : $t('json_n_keys', { n: row.childCount }) }}
                  </span>
                </template>
              </template>
            </template>

            <template v-else-if="row.kind === 'close'">
              <span v-if="xml && (row.key || xmlRoot)" class="jv-key-xml">&lt;/{{ row.key || xmlRoot }}&gt;</span>
              <template v-else-if="!xml">
                <span class="jv-punct">{{ row.isArray ? ']' : '}' }}</span>
                <span v-if="row.trailingComma" class="jv-punct">,</span>
              </template>
            </template>

            <template v-else>
              <template v-if="xml && row.key.startsWith('@')">
                <span class="jv-key-xml jv-attr" :class="{ 'jv-match': row.match }">{{ row.key }}</span>
                <span class="jv-punct">=</span>
                <span class="jv-value jv-string">"{{ row.value }}"</span>
              </template>
              <template v-else>
                <span v-if="row.showKey" class="jv-key" :class="{ 'jv-match': row.match }">"{{ row.key }}"</span>
                <span v-if="row.showKey" class="jv-punct">:&nbsp;</span>
                <span class="jv-value" :class="valueClass(row.value)">{{ displayValue(row.value) }}</span>
                <span v-if="row.trailingComma" class="jv-punct">,</span>
                <span v-if="!xml && timestampFor(row)" class="jv-ts" :title="String(timestampFor(row)!.original)">
                  <i-lucide-clock />
                  {{ timestampFor(row)!.formatted }}
                </span>
              </template>
              <button
                class="jv-copy"
                type="button"
                :title="row.path"
                :aria-label="$t('copy_path')"
                @click.stop="copyPath(row.path)"
              >
                {{ copiedPath === row.path ? '✓' : row.path }}
              </button>
            </template>
          </div>
        </div>
      </div>
    </template>
    <div v-else class="json-tree-empty">{{ $t('no_data') }}</div>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref, shallowRef, watch } from 'vue'
import { buildRows, isNodeCollapsed, type TreeRow } from './rows'
import { detectTimestamp, type TimestampInfo } from './timestamp'

const props = withDefaults(
  defineProps<{
    value: unknown
    expandDepth?: number
    xml?: boolean
    xmlRoot?: string
    filterPaths?: Set<string> | null
  }>(),
  { expandDepth: 1, xml: false, xmlRoot: '', filterPaths: null },
)

const ROW_HEIGHT = 24
const BUFFER = 5

const scrollRef = ref<HTMLDivElement>()
const scrollTop = ref(0)
const containerHeight = ref(0)
const copiedPath = ref('')
let copyTimer: ReturnType<typeof setTimeout> | undefined
let resizeObserver: ResizeObserver | undefined
let scrollFrame = 0

const expandedPaths = reactive(new Set<string>())
const collapsedPaths = reactive(new Set<string>())

const rows = shallowRef<TreeRow[]>([])

const hasData = computed(() => props.value !== null && props.value !== undefined)

const totalHeight = computed(() => rows.value.length * ROW_HEIGHT)
const startIndex = computed(() => Math.max(0, Math.floor(scrollTop.value / ROW_HEIGHT) - BUFFER))
const endIndex = computed(() =>
  Math.min(rows.value.length, startIndex.value + Math.ceil(containerHeight.value / ROW_HEIGHT) + BUFFER * 2),
)
const startOffset = computed(() => startIndex.value * ROW_HEIGHT)
const visibleRows = computed(() => rows.value.slice(startIndex.value, endIndex.value))

function rebuild() {
  rows.value = buildRows(props.value, {
    expanded: expandedPaths,
    collapsed: collapsedPaths,
    expandDepth: props.expandDepth,
    filterPaths: props.filterPaths ?? undefined,
  })
}

watch(
  () => [props.value, props.expandDepth, props.filterPaths, props.xml] as const,
  () => {
    expandedPaths.clear()
    collapsedPaths.clear()
    rebuild()
  },
  { immediate: true },
)

function togglePath(path: string, depth: number) {
  const state = {
    expanded: expandedPaths,
    collapsed: collapsedPaths,
    expandDepth: props.expandDepth,
    filterPaths: props.filterPaths ?? undefined,
  }
  if (isNodeCollapsed(path, depth, state)) {
    expandedPaths.add(path)
    collapsedPaths.delete(path)
  } else {
    collapsedPaths.add(path)
    expandedPaths.delete(path)
  }
  rebuild()
}

function onScroll() {
  cancelAnimationFrame(scrollFrame)
  scrollFrame = requestAnimationFrame(() => {
    if (scrollRef.value) scrollTop.value = scrollRef.value.scrollTop
  })
}

onMounted(() => {
  const el = scrollRef.value
  if (!el) return
  containerHeight.value = el.clientHeight
  if (typeof ResizeObserver === 'undefined') return
  resizeObserver = new ResizeObserver(entries => {
    containerHeight.value = entries[0]?.contentRect.height ?? 0
  })
  resizeObserver.observe(el)
})

onUnmounted(() => {
  resizeObserver?.disconnect()
  cancelAnimationFrame(scrollFrame)
  clearTimeout(copyTimer)
})

function displayValue(value: unknown): string {
  if (value === null) return 'null'
  if (value === undefined) return 'undefined'
  if (typeof value === 'string') return `"${value}"`
  return String(value)
}

function valueClass(value: unknown): string {
  if (value === null || value === undefined) return 'jv-null'
  if (typeof value === 'string') return 'jv-string'
  if (typeof value === 'number') return 'jv-number'
  if (typeof value === 'boolean') return 'jv-boolean'
  return ''
}

function timestampFor(row: TreeRow): TimestampInfo | null {
  if (row.kind !== 'value') return null
  return detectTimestamp(row.value)
}

async function copyPath(path: string) {
  try {
    await navigator.clipboard.writeText(path)
    copiedPath.value = path
  } catch {
    copiedPath.value = ''
    return
  }
  clearTimeout(copyTimer)
  copyTimer = setTimeout(() => {
    copiedPath.value = ''
  }, 1500)
}
</script>

<style lang="scss" scoped>
.json-tree {
  --jv-key: #7e22ce;
  --jv-string: #15803d;
  --jv-number: #2563eb;
  --jv-boolean: #d97706;
  --jv-ts-fg: #b45309;
  --jv-ts-bg: #fef3c7;

  height: 100%;
  overflow: auto;
  color: var(--md-sys-color-on-surface);
  font-family: ui-monospace, SFMono-Regular, SF Mono, Menlo, Consolas, Liberation Mono, monospace;
  font-size: 0.875rem;
  line-height: 1.5;
  user-select: text;

  :root.dark & {
    --jv-key: #c084fc;
    --jv-string: #4ade80;
    --jv-number: #60a5fa;
    --jv-boolean: #fbbf24;
    --jv-ts-fg: #fcd34d;
    --jv-ts-bg: rgba(146, 64, 14, 0.32);
  }
}

.json-tree-spacer {
  position: relative;
}

.json-tree-window {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
}

.tree-row {
  display: flex;
  align-items: center;
  padding: 0 4px;
  margin: 0 -4px;
  border-radius: 4px;
  white-space: nowrap;

  &:hover {
    background: color-mix(in srgb, var(--md-sys-color-primary) 8%, transparent);

    .jv-hint,
    .jv-ts,
    .jv-copy {
      opacity: 1;
    }
  }
}

.tree-row--open {
  cursor: pointer;
}

.jv-indent {
  display: inline-block;
  flex-shrink: 0;
}

.jv-arrow {
  width: 16px;
  height: 20px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  color: var(--md-sys-color-outline);

  svg {
    width: 12px;
    height: 12px;
    fill: currentColor;
    transition: transform 0.15s;
  }

  &.open svg {
    transform: rotate(90deg);
  }
}

.jv-key {
  color: var(--jv-key);
}

.jv-key-xml {
  color: var(--jv-number);

  &.jv-attr {
    color: var(--jv-key);
  }
}

.jv-match {
  background: var(--jv-ts-bg);
  border-radius: 3px;
}

.jv-punct {
  color: var(--md-sys-color-on-surface-variant);
}

.jv-muted {
  color: var(--md-sys-color-outline);
  margin: 0 4px;
}

.jv-hint {
  margin-left: 8px;
  font-size: 0.75rem;
  color: var(--md-sys-color-outline);
  opacity: 0;
  transition: opacity 0.15s;
}

.jv-value {
  &.jv-string {
    color: var(--jv-string);
  }

  &.jv-number {
    color: var(--jv-number);
  }

  &.jv-boolean {
    color: var(--jv-boolean);
    font-weight: 500;
  }

  &.jv-null {
    color: var(--md-sys-color-outline);
    font-style: italic;
  }
}

.jv-ts {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-left: 8px;
  padding: 0 4px;
  border-radius: 4px;
  font-size: 0.75rem;
  color: var(--jv-ts-fg);
  background: var(--jv-ts-bg);
  opacity: 0.8;
  transition: opacity 0.15s;

  svg {
    width: 12px;
    height: 12px;
  }
}

.jv-copy {
  margin-left: auto;
  flex-shrink: 0;
  padding: 0 0 0 8px;
  border: none;
  background: none;
  font: inherit;
  font-size: 0.75rem;
  color: var(--md-sys-color-outline);
  opacity: 0;
  cursor: pointer;
  transition: opacity 0.15s, color 0.15s;

  &:hover {
    color: var(--md-sys-color-primary);
  }
}

.json-tree-empty {
  padding: 32px 16px;
  text-align: center;
  color: var(--md-sys-color-on-surface-variant);
  font-style: italic;
}
</style>
